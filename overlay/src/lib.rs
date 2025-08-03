mod card_display;
pub mod hero_display;

use card_display::CardDisplayManager;
use indicatif::ProgressBar;

use lib::{
    fade::{convert_alpha_to_white, remove_color, remove_white_corners},
    image::{load_image, load_image_unchanged, FullArtHeroManager},
    intro::{generate_intro, VideoCapLooper, INTRO_TIME},
    life_tracker::LifeTracker,
    relative_roi::{HorizontalPartition, RelativeRoi, VerticalPartition},
    text::{center_text_at_rect, center_text_at_rel},
};
use opencv::{
    core::{self, flip, Rect, Scalar, Size, UMat, UMatTrait, UMatTraitConst},
    imgproc::{
        self, cvt_color_def, COLOR_RGBA2RGB, FONT_HERSHEY_SCRIPT_COMPLEX, FONT_HERSHEY_SIMPLEX,
        LINE_8,
    },
    videoio::{
        self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter,
        VideoWriterTrait, CAP_PROP_FRAME_COUNT,
    },
};
use serde::Deserialize;
use std::{borrow::BorrowMut, collections::VecDeque, error, ops::Sub};
use tempfile::NamedTempFile;

// Card display
const DISPLAY_DURATION: f64 = 6.0;
const EXTENDED_DISPLAY_DURATION: f64 = 12.0;
const FADE_OUT_DURATION: f64 = 0.75;
const ROTATE_TIME: f64 = 0.75;
const ZOOM_TIME: f64 = 2.0;
const ZOOM_DISPLAY: f64 = 3.0;
const POST_ZOOM_TIME: f64 = 1.0;

// Constants
const MILLI: f64 = 1_000.0;
const FRAME_WIDTH: i32 = 1920;
const FRAME_HEIGHT: i32 = 1080;

// Colors
const GREEN: Scalar = Scalar::new(0.0, 255.0, 0.0, 0.0);
const WHITE: Scalar = Scalar::new(255.0, 255.0, 255.0, 0.0);

// Background
const BACKGROUND_ANIM_FILE: &'static str = "data/hexagon.mp4";

// Scoreboard dimensions
const SCOREBOARD_WIDTH_RATIO: f64 = 0.2;

// Relative dimensions
const TOP_PANEL_HEIGHT_RATIO: f64 = 1.0 / 8.0;
const WIDTH_BUFFER_RATIO: f64 = 1.0 / 100.0;
const HEIGHT_BUFFER_RATIO: f64 = 1.0 / 100.0;
const SIDE_PANEL_WIDTH_RATIO: f64 = 1.0 / 5.0;
const LIFE_SYMBOL_WIDTH_RATIO: f64 = 1.0 / 30.0;

// Fonts
const SCORE_FONT_SCALE: f64 = 10.0;
const SCORE_FONT_STYLE: i32 = FONT_HERSHEY_SCRIPT_COMPLEX;
const SCORE_FONT_WIDTH: i32 = 10;

const TURN_FONT_SCALE: f64 = 1.75;
const TURN_FONT_FACE: i32 = FONT_HERSHEY_SIMPLEX;
const TURN_FONT_THICKNESS: i32 = 3;

// Heros
// const HERO_OFFSET_RATIO: f64 = 1.0 / 256.0;
const HERO_BORDER_THICKNESS: i32 = 5;
const HERO_TURN_COLOR: Scalar = Scalar::new(0.0, 100.0, 255.0, 0.0);
const HERO_WIN_COLOR: Scalar = Scalar::new(0.0, 255.0, 0.0, 0.0);
const HERO_DEF_COLOR: Scalar = Scalar::new(0.0, 0.0, 0.0, 0.0);

// Life
const LIFE_TICK: f64 = 250.0;

// File Constants
const PLAYER1_DATA_TYPE: &str = "player1";
const LIFE_DATA_TYPE: &str = "life";
const CARD_DATA_TYPE: &str = "card";
const TURN_DATA_TYPE: &str = "turn";
const ZOOM: &str = "zoom";

// Logo
const LOGO_FP: &str = "data/image.png";
const CARD_BACK_FP: &str = "data/cardback.png";
const LIFE_FP: &'static str = "data/life.png";

// Change the alias to use `Box<dyn error::Error>`.
type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

#[derive(Deserialize, Debug, Default)]
struct DataRow {
    sec: u64,
    milli: f64,
    name: String,
    pitch: Option<u32>,
    player1_life: Option<String>,
    player2_life: Option<String>,
    update_type: String,
}

#[derive(Clone, Copy, Debug)]
struct TimeTick {
    sec: u64,
    milli: f64,
}

impl TimeTick {
    fn new() -> Self {
        TimeTick { sec: 0, milli: 0.0 }
    }

    fn build(sec: u64, milli: f64) -> Self {
        TimeTick { sec, milli }
    }

    fn increment_milli(&mut self, increment: f64) {
        self.milli += increment;
        if self.milli > MILLI {
            self.sec += 1;
            self.milli -= MILLI;
        }
    }

    fn as_f64(&self) -> f64 {
        self.sec as f64 + (self.milli / MILLI)
    }
}

impl Sub for TimeTick {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        if self.milli < rhs.milli {
            TimeTick {
                sec: (self.sec - 1) - rhs.sec,
                milli: (self.milli + MILLI) - rhs.milli,
            }
        } else {
            TimeTick {
                sec: self.sec - rhs.sec,
                milli: self.milli - rhs.milli,
            }
        }
    }
}

impl PartialEq for TimeTick {
    fn eq(&self, other: &Self) -> bool {
        (self.sec, self.milli) == (other.sec, other.milli)
    }

    fn ne(&self, other: &Self) -> bool {
        (self.sec, self.milli) != (other.sec, other.milli)
    }
}

impl PartialOrd for TimeTick {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (self.sec, self.milli).partial_cmp(&(other.sec, other.milli))
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TurnPlayer {
    None,
    One,
    Two,
}

impl TurnPlayer {
    fn swap_update(&mut self, default: &Self) {
        match &self {
            Self::One => {
                *self = Self::Two;
            }
            Self::Two => {
                *self = Self::One;
            }
            Self::None => *self = default.clone(),
        }
    }
}

pub fn run(video_fp: &str, annotation_fp: &str, output_fp: &str, timeout: Option<u64>) -> Result<()> {
    // Load game stats
    let mut rows: VecDeque<std::result::Result<DataRow, csv::Error>> = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(annotation_fp)
        .expect("Could not load card file")
        .deserialize()
        .collect();

    let fst_player_row = rows
        .pop_front()
        .expect("Invalid card file")
        .expect("Invalid row format");
    let snd_player_row = rows
        .pop_front()
        .expect("Invalid card file")
        .expect("Invalid row format");
    let (player1, player2) = {
        if fst_player_row.update_type == PLAYER1_DATA_TYPE {
            (fst_player_row.name, snd_player_row.name)
        } else {
            (snd_player_row.name, fst_player_row.name)
        }
    };

    let first_stats = rows
        .pop_front()
        .expect("Invalid card file")
        .expect("Invalid row format");
    let second_stats = rows
        .pop_front()
        .expect("Invalid card file")
        .expect("Invalid row format");

    let first_turn_player = {
        if first_stats.player1_life.is_some() {
            TurnPlayer::One
        } else {
            TurnPlayer::Two
        }
    };

    let mut turn_player = TurnPlayer::None;

    let (hero1_stats, hero2_stats) = {
        if first_turn_player == TurnPlayer::One {
            (first_stats, second_stats)
        } else {
            (second_stats, first_stats)
        }
    };

    let tmp_file = NamedTempFile::new()?;
    let tmp_path = tmp_file.path().to_str().unwrap();

    // Create capture
    let mut cap = VideoCapture::from_file(video_fp, videoio::CAP_ANY)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;

    // Create background capture
    let mut background_loop = VideoCapLooper::build(&BACKGROUND_ANIM_FILE)?;

    let frame_size = Size::new(FRAME_WIDTH, FRAME_HEIGHT);

    // Relative dimensions

    // Top panel
    let hero1_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO,
        0.0,
        (1.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO,
        WIDTH_BUFFER_RATIO,
        0.0,
        HEIGHT_BUFFER_RATIO,
        0.0,
    )?;
    let hero2_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO + (2.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        0.0,
        (1.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO,
        0.0,
        WIDTH_BUFFER_RATIO,
        HEIGHT_BUFFER_RATIO,
        0.0,
    )?;
    let player1_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO,
        TOP_PANEL_HEIGHT_RATIO,
        (1.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO / 4.0,
        WIDTH_BUFFER_RATIO,
        0.0,
        0.0,
        0.0,
    )?;
    let player2_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO + (2.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO,
        (1.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO / 4.0,
        0.0,
        WIDTH_BUFFER_RATIO,
        0.0,
        0.0,
    )?;
    let life1_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO + (1.0 / 3.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        0.0,
        (1.0 / 6.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO,
        0.0,
        WIDTH_BUFFER_RATIO,
        HEIGHT_BUFFER_RATIO,
        0.0,
    )?;
    let life2_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO + 0.5 * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        0.0,
        (1.0 / 6.0) * (1.0 - SIDE_PANEL_WIDTH_RATIO),
        TOP_PANEL_HEIGHT_RATIO,
        WIDTH_BUFFER_RATIO,
        0.0,
        HEIGHT_BUFFER_RATIO,
        0.0,
    )?;
    let life_symbol_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO + (1.0 - SIDE_PANEL_WIDTH_RATIO) * 0.5
            - LIFE_SYMBOL_WIDTH_RATIO / 2.0,
        0.0,
        LIFE_SYMBOL_WIDTH_RATIO,
        TOP_PANEL_HEIGHT_RATIO,
        0.0,
        0.0,
        HEIGHT_BUFFER_RATIO,
        0.0,
    )?;

    // Inner frame
    let innerframe_rel_roi = RelativeRoi::build(
        SIDE_PANEL_WIDTH_RATIO,
        TOP_PANEL_HEIGHT_RATIO,
        1.0 - SIDE_PANEL_WIDTH_RATIO,
        1.0 - TOP_PANEL_HEIGHT_RATIO,
        WIDTH_BUFFER_RATIO / 2.0,
        WIDTH_BUFFER_RATIO,
        HEIGHT_BUFFER_RATIO,
        HEIGHT_BUFFER_RATIO,
    )?;

    // Side panel
    let logo_rel_roi = RelativeRoi::build_as_partition(
        0.0,
        0.0,
        SCOREBOARD_WIDTH_RATIO,
        0.5,
        Some(WIDTH_BUFFER_RATIO),
        Some(HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Top),
    )?;
    let card_rel_roi = RelativeRoi::build_as_partition(
        0.0,
        0.5,
        SIDE_PANEL_WIDTH_RATIO,
        0.5,
        Some(WIDTH_BUFFER_RATIO),
        Some(HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Bottom),
    )?;

    // Get hero images
    let full_art_manager = FullArtHeroManager::new();
    let hero1_animation_fp = full_art_manager.get_hero_art_animation_fp(&hero1_stats.name)?;
    let hero2_animation_fp = full_art_manager.get_hero_art_animation_fp(&hero2_stats.name)?;

    let mut hero1_animation = VideoCapLooper::build(&hero1_animation_fp)?;
    let mut hero2_animation = VideoCapLooper::build(&hero2_animation_fp)?;

    // Load card back
    let card_back_img = load_image(&CARD_BACK_FP)?;
    let green_background =
        UMat::new_size_with_default_def(card_back_img.size()?, card_back_img.typ(), GREEN)?;
    let card_back_img = remove_white_corners(&green_background, &card_back_img)?;
    let card_back_img = card_rel_roi.resize(&frame_size, &card_back_img)?;
    let card_rect = card_rel_roi.generate_roi(&frame_size, &card_back_img);

    let increment = fps.recip() * MILLI;

    // Generate output video
    let mut out = VideoWriter::new(
        &tmp_path,
        // VideoWriter::fourcc('h', '2', '6', '4').unwrap(),
        // VideoWriter::fourcc('a', 'v', 'c', '1').unwrap(),
        VideoWriter::fourcc('m', 'p', '4', 'v').unwrap(),
        fps,
        frame_size,
        true,
    )?;

    // Create intro
    println!("Generating intro...");
    generate_intro(
        &hero1_animation_fp,
        &player1,
        &hero2_animation_fp,
        &player2,
        &frame_size,
        card_back_img.typ(),
        fps,
        &mut out,
    )?;
    println!("Intro generated!");

    // Load GoToOne Logo
    let logo_image = load_image(&LOGO_FP)?;
    let mut logo_image = logo_rel_roi.resize(&frame_size, &logo_image)?;
    let logo_roi = logo_rel_roi.generate_roi(&frame_size, &logo_image);
    imgproc::rectangle(
        &mut logo_image,
        core::Rect::new(0, 0, logo_roi.width, logo_roi.height),
        Scalar::new(0., 0., 0., 0.),
        (HERO_BORDER_THICKNESS as f64 * 2.0) as i32,
        imgproc::LINE_8,
        0,
    )?;

    // stop further mutations
    let logo_image = logo_image;

    // Set init vars
    let mut time_tick = TimeTick::new();
    let mut winner: Option<u8> = None;

    // Track what the players lives should be so we can tick them down
    let mut player1_life_tracker =
        LifeTracker::build(&hero1_stats.player1_life.unwrap(), LIFE_TICK, increment);
    let mut player2_life_tracker =
        LifeTracker::build(&hero2_stats.player2_life.unwrap(), LIFE_TICK, increment);

    let mut turn_counter = 0_u32;

    // start progress bar
    let bar = {
        if timeout.is_some() {
            ProgressBar::new(((timeout.unwrap() + 1) as f64 * MILLI) as u64)
        } else {
            ProgressBar::new(cap.get(CAP_PROP_FRAME_COUNT).unwrap() as u64)
        }
    };

    let mut card_display_manager = CardDisplayManager::new(&card_rect, &card_back_img, &time_tick);

    // Cut beginning of video where intro would be
    for _ in 0..(INTRO_TIME * fps) as i32 {
        let mut frame = UMat::new_def();
        cap.read(&mut frame)?;
        time_tick.increment_milli(increment);
    }

    // LOOP HERE
    println!("overlaying video...");
    loop {
        // Check timeout
        if let Some(sec) = timeout {
            if time_tick.sec > sec {
                break;
            }
        }

        let mut frame = UMat::new_def();
        time_tick.increment_milli(increment);

        // Increment life ticker
        player1_life_tracker.tick_display();
        player2_life_tracker.tick_display();
        
        // Grab frame
        if !cap.read(&mut frame).unwrap_or(false) {
            break;
        }

        // Draw background
        let background_frame = background_loop.background_read()?;

        let mut background = UMat::new_def();
        opencv::imgproc::resize(
            &background_frame,
            &mut background,
            frame_size,
            0.0,
            0.0,
            opencv::imgproc::INTER_AREA,
        )?;


        let mut innerframe = UMat::new_def();
        frame.copy_to(&mut innerframe)?;

        let reframe = innerframe_rel_roi.resize(&frame_size, &innerframe)?;
        let frame_roi_rect = innerframe_rel_roi.generate_roi(&frame_size, &innerframe);
        let mut frame_roi = background.roi_mut(frame_roi_rect)?;
        reframe.copy_to(frame_roi.borrow_mut())?;
        imgproc::rectangle(
            &mut background,
            frame_roi_rect,
            Scalar::new(0.0, 0.0, 0.0, 0.0),
            10, // Thickness of -1 fills the rectangle completely
            LINE_8,
            0,
        )?;

        // quick fix
        frame = background;

        // Heroes
        let hero1_image = hero1_animation.read()?;
        let mut hero1_image = FullArtHeroManager::crop_hero_img(&hero1_image)?;
        flip(&hero1_image.clone(), &mut hero1_image, 1)?;
        let hero1_rect = hero1_rel_roi.generate_roi(&frame_size, &hero1_image);
        let hero1_image = hero1_rel_roi.resize(&frame_size, &hero1_image)?;

        let mut hero1_roi = frame.roi_mut(hero1_rect)?;
        hero1_image.copy_to(hero1_roi.borrow_mut())?;
        let hero1_color = {
            if winner.is_some_and(|v| v == 1) {
                HERO_WIN_COLOR
            } else if turn_player == TurnPlayer::One {
                HERO_TURN_COLOR
            } else {
                HERO_DEF_COLOR
            }
        };
        imgproc::rectangle(
            &mut frame,
            hero1_rect,
            hero1_color,
            HERO_BORDER_THICKNESS,
            imgproc::LINE_8,
            0,
        )?;

        let hero2_image = hero2_animation.read()?;
        let hero2_image = FullArtHeroManager::crop_hero_img(&hero2_image)?;
        let hero2_rect = hero2_rel_roi.generate_roi(&frame_size, &hero2_image);
        let hero2_image = hero2_rel_roi.resize(&frame_size, &hero2_image)?;

        let mut hero2_roi = frame.roi_mut(hero2_rect)?;
        hero2_image.copy_to(hero2_roi.borrow_mut())?;

        let hero2_color = {
            if winner.is_some_and(|v| v == 2) {
                HERO_WIN_COLOR
            } else if turn_player == TurnPlayer::Two {
                HERO_TURN_COLOR
            } else {
                HERO_DEF_COLOR
            }
        };
        imgproc::rectangle(
            &mut frame,
            hero2_rect,
            hero2_color,
            HERO_BORDER_THICKNESS,
            imgproc::LINE_8,
            0,
        )?;

        let left_rect = life1_rel_roi.generate_roi_raw(&frame_size);
        let right_rect = life2_rel_roi.generate_roi_raw(&frame_size);

        let mut overlay = frame.clone();
        imgproc::rectangle(
            &mut overlay,
            left_rect,
            Scalar::new(0., 0., 0., 0.),
            -1,
            imgproc::LINE_8,
            0,
        )?;
        core::add_weighted(&overlay, 0.5, &frame.clone(), 0.5, 0., &mut frame, -1)?;

        let mut overlay = frame.clone();
        imgproc::rectangle(
            &mut overlay,
            right_rect,
            Scalar::new(0., 0., 0., 0.),
            -1,
            imgproc::LINE_8,
            0,
        )?;
        core::add_weighted(&overlay, 0.5, &frame.clone(), 0.5, 0., &mut frame, -1)?;

        center_text_at_rel(
            &mut frame,
            &player1_life_tracker.display(),
            SCORE_FONT_STYLE,
            SCORE_FONT_SCALE,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            SCORE_FONT_WIDTH,
            life1_rel_roi,
            20,
        )?;
        center_text_at_rel(
            &mut frame,
            &player2_life_tracker.display(),
            SCORE_FONT_STYLE,
            SCORE_FONT_SCALE,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            SCORE_FONT_WIDTH,
            life2_rel_roi,
            20,
        )?;
        center_text_at_rel(
            &mut frame,
            &player1,
            TURN_FONT_FACE,
            TURN_FONT_SCALE,
            WHITE,
            TURN_FONT_THICKNESS,
            player1_rel_roi,
            20,
        )?;
        center_text_at_rel(
            &mut frame,
            &player2,
            TURN_FONT_FACE,
            TURN_FONT_SCALE,
            WHITE,
            TURN_FONT_THICKNESS,
            player2_rel_roi,
            20,
        )?;

        // Life
        let life_img = load_image_unchanged(LIFE_FP)?;
        let mut life_img = convert_alpha_to_white(&life_img)?;
        cvt_color_def(&life_img.clone(), &mut life_img, COLOR_RGBA2RGB)?;

        let life_rect = life_symbol_rel_roi.generate_roi(&frame_size, &life_img);
        let life_img = life_symbol_rel_roi.resize(&frame_size, &life_img)?;

        let roi = frame.roi(life_rect)?;
        let new = remove_color(&roi, &life_img, &Scalar::new(255.0, 255.0, 255.0, 0.0))?;

        let mut roi = frame.roi_mut(life_rect)?;
        new.copy_to(roi.borrow_mut())?;

        // Turn counter
        if turn_counter > 0 {
            let turn_counter_rect = Rect::new(
                frame_roi_rect.x + 7 * frame_roi_rect.width.div_euclid(8),
                frame_roi_rect.y,
                frame_roi_rect.width.div_euclid(8),
                frame_roi_rect.height.div_euclid(16),
            );
            imgproc::rectangle(
                &mut frame,
                turn_counter_rect,
                Scalar::new(0., 0., 0., 0.),
                -1,
                imgproc::LINE_8,
                0,
            )?;
            center_text_at_rect(
                &mut frame,
                &format!("Turn {}", turn_counter),
                TURN_FONT_FACE,
                TURN_FONT_SCALE,
                Scalar::new(255.0, 255.0, 255.0, 0.0),
                TURN_FONT_THICKNESS,
                turn_counter_rect,
                20,
            )?;
        }

        let mut logo_roi = frame.roi_mut(logo_roi)?;
        logo_image.copy_to(logo_roi.borrow_mut())?;

        // Parse Row Data
        if let Some(row) = rows.front() {
            let row = row.as_ref().expect("Invalid row data");
            let time = TimeTick::build(row.sec, row.milli);
            // Card time just passed
            if time <= time_tick {
                let row = rows.pop_front().unwrap().unwrap();
                if row.update_type.trim() == CARD_DATA_TYPE {
                    card_display_manager.add_card_to_queue(row);
                } else if row.update_type == ZOOM {
                    card_display_manager.queue_zoom();
                } else if row.update_type == TURN_DATA_TYPE {
                    turn_counter += 1;
                    turn_player.swap_update(&first_turn_player);
                } else if row.update_type == LIFE_DATA_TYPE {
                    if let Some(update) = row.player1_life {
                        player1_life_tracker.update(&update);
                    }
                    if let Some(update) = row.player2_life {
                        player2_life_tracker.update(&update);
                    }
                } else {
                    if row.update_type == "win1" {
                        let _ = winner.insert(1);
                    } else {
                        let _ = winner.insert(2);
                    }
                }
            }
        }

        card_display_manager.tick(time_tick, &mut frame, &frame_roi_rect)?;

        out.write(&frame)?;
        if timeout.is_some() {
            bar.inc(increment as u64);
        } else {
            bar.inc(1);
        }
    }

    // end progress bar
    bar.finish();
    out.release()?;

    std::fs::copy(tmp_path, output_fp)?;

    Ok(())
}

// // Crop frame
// let crop_left =
//     ((crop_left.unwrap_or(0.0) / 100.0) * frame.size()?.width as f64) as i32;
// let crop_right =
//     ((args.crop_right.unwrap_or(0.0) / 100.0) * frame.size()?.width as f64) as i32;
// let crop_top =
//     ((args.crop_top.unwrap_or(0.0) / 100.0) * frame.size()?.height as f64) as i32;
// let crop_bottom =
//     ((args.crop_bottom.unwrap_or(0.0) / 100.0) * frame.size()?.height as f64) as i32;

// let crop_roi = frame.roi(core::Rect::new(
//     crop_left,
//     crop_top,
//     frame.size()?.width - (crop_left + crop_right),
//     ((frame.size()?.height - (crop_top + crop_bottom)) as f64 * FRAME_HEIGHT_RATIO) as i32,
// ))?;
