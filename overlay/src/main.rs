use clap::Parser;
use indicatif::ProgressBar;

use lib::{
    fade::{remove_color, remove_white_corners},
    image::get_card_art,
    life_tracker::LifeTracker,
    relative_roi::{HorizontalPartition, RelativeRoi, VerticalPartition},
    rotate::rotate_image,
    text::center_text_at,
};
use opencv::{
    core::{self, bitwise_not_def, MatTraitConst, Scalar, Size, UMat, UMatTrait, UMatTraitConst},
    imgcodecs,
    imgproc::{
        self, cvt_color_def, COLOR_BGR2GRAY, COLOR_GRAY2RGB, FONT_HERSHEY_SCRIPT_COMPLEX,
        FONT_HERSHEY_SIMPLEX, LINE_8,
    },
    videoio::{
        self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter,
        VideoWriterTrait, CAP_PROP_FRAME_COUNT,
    },
};
use serde::Deserialize;
use std::{borrow::BorrowMut, collections::VecDeque, error, ops::Sub, process::Command};
use tempfile::NamedTempFile;

// Card display
const FADE_IN_DURATION: f64 = 0.75;
const DISPLAY_DURATION: f64 = 6.0;
const EXTENDED_DISPLAY_DURATION: f64 = 12.0;
const FADE_OUT_DURATION: f64 = 0.75;

// Constants
// const CARD_WIDTH_RATIO: f64 = 450.0 / 628.0;
const CARD_HEIGHT_RATIO: f64 = 628.0 / 450.0;
const MILLI: f64 = 1_000.0;

// Background
const BACKGROUND_ANIM_FILE: &'static str = "data/hexagon.mp4";

// Frame dimensions
// const FRAME_WIDTH_RATIO: f64 = 1.0 - (1.0 / 64.0);
const FRAME_HEIGHT_RATIO: f64 = 1.0 - (1.0 / 64.0);

// Scoreboard dimensions
const SCOREBOARD_WIDTH_RATIO: f64 = 0.2;
const SCOREBOARD_HEIGHT_BUFFER_RATIO: f64 = 0.02;
const SCOREBOARD_WIDTH_BUFFER_RATIO: f64 = 0.01;

// Fonts
const SCORE_FONT_SCALE: f64 = 1.75;
const SCORE_FONT_STYLE: i32 = FONT_HERSHEY_SCRIPT_COMPLEX;
const SCORE_FONT_WIDTH: i32 = 3;

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
const LIFE_DATA_TYPE: &str = "life";
const CARD_DATA_TYPE: &str = "card";
const TURN_DATA_TYPE: &str = "turn";

// Logo
const LOGO_FP: &str = "data/image.png";
const CARD_BACK_FP: &str = "data/cardback.png";

// Change the alias to use `Box<dyn error::Error>`.
type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

#[derive(Deserialize, Debug)]
struct DataRow {
    sec: u64,
    milli: f64,
    name: String,
    pitch: Option<u32>,
    player1_life: Option<String>,
    player2_life: Option<String>,
    update_type: String,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    video_file: String,

    #[arg(short, long)]
    card_file: String,

    #[arg(short, long)]
    timeout: Option<u64>,

    #[arg(short, long, action)]
    debug: bool,

    #[arg(long)]
    crop_left: Option<f64>,

    #[arg(long)]
    crop_right: Option<f64>,

    #[arg(long)]
    crop_top: Option<f64>,

    #[arg(long)]
    crop_bottom: Option<f64>,

    #[arg(long)]
    output_file: Option<String>,
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

#[derive(Debug, PartialEq, Eq)]
enum FadeStage {
    PRE,
    IN,
    DISPLAY,
    OUT,
    POST,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum TurnPlayer {
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

struct VideoCapLooper {
    fp: String,
    cap: VideoCapture,
}

impl VideoCapLooper {
    fn build(video_fp: &str) -> Result<Self> {
        let cap = VideoCapture::from_file_def(video_fp)?;
        Ok(Self {
            fp: video_fp.to_owned(),
            cap,
        })
    }

    fn read(&mut self) -> Result<UMat> {
        let mut frame = UMat::new_def();
        let got = self.cap.read(&mut frame)?;
        if !got {
            self.cap = VideoCapture::from_file_def(&self.fp)?;
            self.cap.read(&mut frame)?;
        }

        // HACK
        let mut inverted_frame = UMat::new_def();
        bitwise_not_def(&frame, &mut inverted_frame)?;
        cvt_color_def(&inverted_frame, &mut frame, COLOR_BGR2GRAY)?;
        // cvt_color_def(&frame, &mut inverted_frame, COLOR_RGB2BGR)?;
        cvt_color_def(&frame, &mut inverted_frame, COLOR_GRAY2RGB)?;

        Ok(inverted_frame)
    }
}

/// Loads image to UMat
fn load_image(fp: &str) -> Result<UMat> {
    let mut umat = UMat::new_def();
    let img = imgcodecs::imread(fp, imgcodecs::IMREAD_COLOR)?;
    img.copy_to(&mut umat)?;

    Ok(umat)
}

fn main() -> Result<()> {
    let args = Cli::parse();

    // Load card back
    let mut card_back_img = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
    let _card_back = imgcodecs::imread(&CARD_BACK_FP, imgcodecs::IMREAD_COLOR).unwrap();
    _card_back.copy_to(&mut card_back_img)?;

    // Load card db
    let card_image_db = lib::card::CardImageDB::init();

    // Load game stats
    let mut rows: VecDeque<std::result::Result<DataRow, csv::Error>> = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(args.card_file)
        .expect("Could not load card file")
        .deserialize()
        .collect();

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

    // Create output
    let output_path = {
        if let Some(out) = args.output_file {
            out
        } else {
            format!("output_videos/{}_output_video.mp4", chrono::Local::now())
        }
    };
    let tmp_file = NamedTempFile::new()?;
    let tmp_path = tmp_file.path().to_str().unwrap();

    // Create capture
    let mut cap = VideoCapture::from_file(&args.video_file, videoio::CAP_ANY)?;
    let original_width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)?;
    let original_height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;

    // Create background capture
    let mut background_loop = VideoCapLooper::build(&BACKGROUND_ANIM_FILE)?;

    let font_scale = { original_width / 1920.0 };

    // Calculate Rotated Dimensions
    let rotate = original_width < original_height;
    let rotated_width = original_height;
    let rotated_height = original_width;

    // Set Frame Dimensions
    let frame_height = if rotate {
        rotated_height
    } else {
        original_height
    };
    let frame_width = if rotate {
        rotated_width
    } else {
        original_width
    };

    let frame_size = Size::new(frame_width as i32, frame_height as i32);

    // Scoreboard dimensions
    let scoreboard_width = (frame_width * SCOREBOARD_WIDTH_RATIO) as i32;

    // Relative dimensions
    let innerframe_rel_roi = RelativeRoi::build_as_partition(
        SCOREBOARD_WIDTH_RATIO,
        0.0,
        1.0 - SCOREBOARD_WIDTH_RATIO,
        1.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Right),
        None,
    )?;
    let turn_counter_rel_roi = RelativeRoi::build(
        1.0 - 1.0 / 10.0,
        0.,
        1.0 / 10.0,
        1.0 / 10.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO + 1.0 / 300.0),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO + 1.0 / 200.0),
    )?;
    let card_rel_roi = RelativeRoi::build_as_partition(
        0.0,
        0.5,
        SCOREBOARD_WIDTH_RATIO,
        0.5,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Bottom),
    )?;
    let logo_rel_roi = RelativeRoi::build_as_partition(
        0.0,
        0.0,
        SCOREBOARD_WIDTH_RATIO,
        3.0 / 12.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Top),
    )?;
    let hero1_rel_roi = RelativeRoi::build_as_partition(
        0.,
        4.0 / 12.0,
        SCOREBOARD_WIDTH_RATIO / 2.0,
        2.0 / 12.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Center),
    )?;
    let hero2_rel_roi = RelativeRoi::build_as_partition(
        SCOREBOARD_WIDTH_RATIO / 2.0,
        4.0 / 12.0,
        SCOREBOARD_WIDTH_RATIO / 2.0,
        2.0 / 12.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Center),
        Some(VerticalPartition::Center),
    )?;
    let left_text_rel_roi = RelativeRoi::build_as_partition(
        0.0,
        3.0 / 12.0,
        SCOREBOARD_WIDTH_RATIO / 2.0,
        1.0 / 12.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Center),
    )?;
    let right_text_rel_roi = RelativeRoi::build_as_partition(
        SCOREBOARD_WIDTH_RATIO / 2.0,
        3.0 / 12.0,
        SCOREBOARD_WIDTH_RATIO / 2.0,
        1.0 / 12.0,
        Some(SCOREBOARD_WIDTH_BUFFER_RATIO),
        Some(SCOREBOARD_HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Center),
        Some(VerticalPartition::Center),
    )?;

    // Get hero images
    let hero1_image = card_image_db.load_card_image(&hero1_stats.name, &None);
    let hero2_image = card_image_db.load_card_image(&hero2_stats.name, &None);

    let hero_width = ((scoreboard_width as f64) * 0.5) as i32;
    let hero_length = (CARD_HEIGHT_RATIO * (hero_width as f64)) as i32;
    let hero1_image =
        get_card_art(&hero1_image, hero_width, hero_length).expect("Could not load hero1 image");
    let hero2_image =
        get_card_art(&hero2_image, hero_width, hero_length).expect("Could not load hero2 image");

    // Card dimensions
    let card_rect = card_rel_roi.generate_roi_raw(&frame_size);

    // Adjust cardback and prevent further mutations
    opencv::imgproc::resize(
        &card_back_img.clone(),
        &mut card_back_img,
        // Size::new(card_width, card_height),
        Size::new(card_rect.width, card_rect.height),
        0.0,
        0.0,
        opencv::imgproc::INTER_LINEAR,
    )?;
    let card_back_img = card_back_img;

    let increment = fps.recip() * MILLI;

    // Generate output video
    let mut out = VideoWriter::new(
        &tmp_path,
        VideoWriter::fourcc('m', 'p', '4', 'v').unwrap(),
        fps,
        Size::new(frame_width as i32, frame_height as i32),
        true,
    )?;

    // Load GoToOne Logo
    let logo_image = load_image(&LOGO_FP)?;
    let mut logo_image = logo_rel_roi.resize(&frame_size, &logo_image)?;
    let logo_roi = logo_rel_roi.generate_roi(&frame_size, &logo_image);
    imgproc::rectangle(
        &mut logo_image,
        core::Rect::new(0, 0, logo_roi.width, logo_roi.height),
        Scalar::new(0., 0., 0., 0.),
        (HERO_BORDER_THICKNESS as f64 * 2.0 * font_scale) as i32,
        imgproc::LINE_8,
        0,
    )?;

    // stop further mutations
    let logo_image = logo_image;

    // Set init vars
    let mut display_start_time = None;
    let mut fade_start_time: Option<TimeTick> = None;
    let mut post_fade_start_time: Option<TimeTick> = None;
    let mut time_tick = TimeTick::new();
    let mut display_card: VecDeque<DataRow> = VecDeque::new();
    let mut card_image = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
    let mut winner: Option<u8> = None;

    // Track what the players lives should be so we can tick them down
    let mut player1_life_tracker =
        LifeTracker::build(&hero1_stats.player1_life.unwrap(), LIFE_TICK, increment);
    let mut player2_life_tracker =
        LifeTracker::build(&hero2_stats.player2_life.unwrap(), LIFE_TICK, increment);

    let mut turn_counter = 0_u32;
    let mut card_back = true;

    // start progress bar
    let bar = {
        if args.timeout.is_some() {
            ProgressBar::new(((args.timeout.unwrap() + 1) as f64 * MILLI) as u64)
        } else {
            ProgressBar::new(cap.get(CAP_PROP_FRAME_COUNT).unwrap() as u64)
        }
    };

    // LOOP HERE
    loop {
        // Check timeout
        if let Some(sec) = args.timeout {
            if time_tick.sec > sec {
                break;
            }
        }

        let mut frame = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
        time_tick.increment_milli(increment);

        // Increment life ticker
        player1_life_tracker.tick_display();
        player2_life_tracker.tick_display();

        // Grab frame
        if !cap.read(&mut frame).unwrap_or(false) {
            break;
        }

        // Draw background
        let background_frame = background_loop.read()?;

        let mut background = UMat::new_def();
        opencv::imgproc::resize(
            &background_frame,
            &mut background,
            Size::new(frame_width as i32, frame_height as i32),
            0.0,
            0.0,
            opencv::imgproc::INTER_AREA,
        )?;

        // Crop frame
        let crop_left = ((args.crop_left.unwrap_or(0.0) / 100.0) * frame_width) as i32;
        let crop_right = ((args.crop_right.unwrap_or(0.0) / 100.0) * frame_width) as i32;
        let crop_top = ((args.crop_top.unwrap_or(0.0) / 100.0) * frame_height) as i32;
        let crop_bottom = ((args.crop_bottom.unwrap_or(0.0) / 100.0) * frame_height) as i32;

        let crop_roi = frame.roi(core::Rect::new(
            crop_left,
            crop_top,
            frame_width as i32 - (crop_left + crop_right),
            ((frame_height - (crop_top + crop_bottom) as f64) * FRAME_HEIGHT_RATIO) as i32,
        ))?;
        let mut innerframe = UMat::new_def();
        crop_roi.copy_to(&mut innerframe)?;

        let reframe = innerframe_rel_roi.resize(&frame_size, &innerframe)?;
        let frame_roi_rect = innerframe_rel_roi.generate_roi(&frame_size, &innerframe);
        let mut frame_roi = background.roi_mut(frame_roi_rect)?;
        // let mut frame_roi = background.roi_mut(frame_rect)?;
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
            HERO_BORDER_THICKNESS * font_scale as i32,
            imgproc::LINE_8,
            0,
        )?;

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
            HERO_BORDER_THICKNESS * font_scale as i32,
            imgproc::LINE_8,
            0,
        )?;

        let left_rect = left_text_rel_roi.generate_roi_raw(&frame_size);
        let right_rect = right_text_rel_roi.generate_roi_raw(&frame_size);

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

        center_text_at(
            &mut frame,
            &player1_life_tracker.display(),
            SCORE_FONT_STYLE,
            SCORE_FONT_SCALE,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            SCORE_FONT_WIDTH,
            left_text_rel_roi,
            20,
        )?;
        center_text_at(
            &mut frame,
            &player2_life_tracker.display(),
            SCORE_FONT_STYLE,
            SCORE_FONT_SCALE,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            SCORE_FONT_WIDTH,
            right_text_rel_roi,
            20,
        )?;

        // Turn counter
        if turn_counter > 0 {
            imgproc::rectangle(
                &mut frame,
                turn_counter_rel_roi.generate_roi_raw(&frame_size),
                Scalar::new(0., 0., 0., 0.),
                -1,
                imgproc::LINE_8,
                0,
            )?;
            center_text_at(
                &mut frame,
                &format!("Turn {}", turn_counter),
                TURN_FONT_FACE,
                TURN_FONT_SCALE,
                Scalar::new(255.0, 255.0, 255.0, 0.0),
                TURN_FONT_THICKNESS,
                turn_counter_rel_roi,
                20,
            )?;
        }

        let mut logo_roi = frame.roi_mut(logo_roi)?;
        logo_image.copy_to(logo_roi.borrow_mut())?;

        // Rotate frame if necessary
        // Not currently working
        if rotate {
            let mut rotated_frame = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
            opencv::core::rotate(
                &frame,
                &mut rotated_frame,
                opencv::core::ROTATE_90_CLOCKWISE,
            )?;
            frame = rotated_frame;
        }

        // Parse Row Data
        if let Some(row) = rows.front() {
            let row = row.as_ref().expect("Invalid row data");
            let time = TimeTick::build(row.sec, row.milli);
            // Card time just passed
            if time <= time_tick {
                let row = rows.pop_front().unwrap().unwrap();
                if row.update_type.trim() == CARD_DATA_TYPE {
                    display_card.push_back(row);
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

        // Add start time and card image
        if let (Some(card), None) = (display_card.front(), display_start_time) {
            display_start_time = Some(time_tick.clone());
            card_image = card_image_db.load_card_image(&card.name, &card.pitch);
        }

        // Display card (rotate)
        if let (Some(_), Some(start_time)) = (&display_card.front(), &display_start_time) {
            let elapsed_time = (time_tick - *start_time).as_f64();
            // card has not faded out yet
            if fade_start_time.is_none_or(|v| (time_tick - v).as_f64() < FADE_OUT_DURATION)
                // Start flip to back
                || (fade_start_time.is_some()
                    && post_fade_start_time.is_none()
                    && display_card.len() == 1)
                // Flip to back in progress
                || post_fade_start_time
                    .is_some_and(|v| (time_tick - v).as_f64() < FADE_OUT_DURATION)
            {
                let fade_stage = {
                    // Fade out card back
                    if elapsed_time < FADE_IN_DURATION && card_back {
                        FadeStage::PRE
                    // Fade in
                    } else if elapsed_time < FADE_IN_DURATION
                        || (elapsed_time < 2.0 * FADE_IN_DURATION && card_back)
                    {
                        FadeStage::IN
                    // Minimum Display time
                    } else if elapsed_time < DISPLAY_DURATION - FADE_OUT_DURATION {
                        FadeStage::DISPLAY
                    // Extended display
                    } else if elapsed_time < EXTENDED_DISPLAY_DURATION - 2.0 * FADE_OUT_DURATION
                        && display_card.len() == 1
                    {
                        FadeStage::DISPLAY
                    // Fade out
                    } else if elapsed_time < EXTENDED_DISPLAY_DURATION - FADE_OUT_DURATION {
                        FadeStage::OUT
                    } else {
                        FadeStage::POST
                    }
                };

                // Start fade out timer if not started yet
                if fade_stage == FadeStage::OUT && fade_start_time.is_none() {
                    card_back = false;
                    let _ = fade_start_time.insert(time_tick.clone());
                }

                // Start post fade out timer if not started yet
                if fade_stage == FadeStage::POST && post_fade_start_time.is_none() {
                    card_back = true;
                    let _ = post_fade_start_time.insert(time_tick.clone());
                }

                if card_image.cols() > card_image.rows() {
                    let mut rotated_card_image = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
                    opencv::core::rotate(
                        &card_image,
                        &mut rotated_card_image,
                        opencv::core::ROTATE_90_CLOCKWISE,
                    )?;
                    card_image = rotated_card_image;
                }

                opencv::imgproc::resize(
                    &card_image.clone(),
                    &mut card_image,
                    // Size::new(card_width, card_height),
                    card_rect.size(),
                    0.0,
                    0.0,
                    opencv::imgproc::INTER_LINEAR,
                )?;

                match fade_stage {
                    FadeStage::PRE => {
                        let alpha = elapsed_time / FADE_IN_DURATION;
                        let green = UMat::new_rows_cols_with_default(
                            card_back_img.rows(),
                            card_back_img.cols(),
                            card_back_img.typ(),
                            Scalar::new(0.0, 255.0, 0.0, 0.0),
                            core::UMatUsageFlags::USAGE_DEFAULT,
                        )?;
                        let card = remove_white_corners(&green, &card_back_img)?;

                        let rotated = rotate_image(&card, alpha as f32, true)?;
                        let rotated_rect = core::Rect::new(
                            card_rect.x,
                            card_rect.y - (rotated.rows() - card_rect.height).div_euclid(2),
                            rotated.cols(),
                            rotated.rows(),
                        );

                        let mut roi = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
                        let _roi = &frame.roi(rotated_rect)?;
                        _roi.copy_to(&mut roi)?;

                        let card_rotation =
                            remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                        let mut inner_roi = frame.roi_mut(rotated_rect)?;
                        card_rotation.copy_to(&mut inner_roi)?;
                    }
                    FadeStage::IN => {
                        let adjusted_lapsed_time = {
                            if card_back {
                                elapsed_time - FADE_IN_DURATION
                            } else {
                                elapsed_time
                            }
                        };

                        let alpha = adjusted_lapsed_time / FADE_IN_DURATION;
                        let green = UMat::new_rows_cols_with_default(
                            card_image.rows(),
                            card_image.cols(),
                            card_image.typ(),
                            Scalar::new(0.0, 255.0, 0.0, 0.0),
                            core::UMatUsageFlags::USAGE_DEFAULT,
                        )?;
                        let card = remove_white_corners(&green, &card_image)?;

                        let rotated = rotate_image(&card, alpha as f32, false)?;
                        let rotated_rect = core::Rect::new(
                            card_rect.x,
                            card_rect.y - (rotated.rows() - card_rect.height).div_euclid(2),
                            rotated.cols(),
                            rotated.rows(),
                        );

                        let mut roi = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
                        let _roi = &frame.roi(rotated_rect)?;
                        _roi.copy_to(&mut roi)?;

                        let card_rotation =
                            remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                        let mut inner_roi = frame.roi_mut(rotated_rect)?;
                        card_rotation.copy_to(&mut inner_roi)?;
                    }
                    FadeStage::DISPLAY => {
                        let mut roi = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
                        let mut inner_roi = frame.roi_mut(card_rect)?;

                        inner_roi.copy_to(&mut roi)?;

                        let card_rotation = remove_white_corners(&roi, &card_image)?;
                        card_rotation.copy_to(&mut inner_roi)?;
                    }
                    FadeStage::OUT => {
                        let alpha =
                            (time_tick - fade_start_time.unwrap()).as_f64() / FADE_OUT_DURATION;
                        let green = UMat::new_rows_cols_with_default(
                            card_image.rows(),
                            card_image.cols(),
                            card_image.typ(),
                            Scalar::new(0.0, 255.0, 0.0, 0.0),
                            core::UMatUsageFlags::USAGE_DEFAULT,
                        )?;
                        let card = remove_white_corners(&green, &card_image)?;
                        let rotated = rotate_image(&card, alpha as f32, true)?;
                        let rotated_rect = core::Rect::new(
                            card_rect.x,
                            card_rect.y - (rotated.rows() - card_rect.height).div_euclid(2),
                            rotated.cols(),
                            rotated.rows(),
                        );

                        let mut roi = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
                        let _roi = &frame.roi(rotated_rect)?;
                        _roi.copy_to(&mut roi)?;

                        let card_rotation =
                            remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                        let card_rotation = remove_white_corners(&roi, &card_rotation)?;

                        let mut inner_roi = frame.roi_mut(rotated_rect)?;
                        card_rotation.copy_to(&mut inner_roi)?;
                    }
                    FadeStage::POST => {
                        let alpha = (time_tick - post_fade_start_time.unwrap()).as_f64()
                            / FADE_OUT_DURATION;
                        let green = UMat::new_rows_cols_with_default(
                            card_image.rows(),
                            card_image.cols(),
                            card_image.typ(),
                            Scalar::new(0.0, 255.0, 0.0, 0.0),
                            core::UMatUsageFlags::USAGE_DEFAULT,
                        )?;
                        let card = remove_white_corners(&green, &card_back_img)?;
                        let rotated = rotate_image(&card, alpha as f32, false)?;
                        let rotated_rect = core::Rect::new(
                            card_rect.x,
                            card_rect.y - (rotated.rows() - card_rect.height).div_euclid(2),
                            rotated.cols(),
                            rotated.rows(),
                        );

                        let mut roi = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
                        let _roi = &frame.roi(rotated_rect)?;
                        _roi.copy_to(&mut roi)?;

                        let card_rotation =
                            remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                        let card_rotation = remove_white_corners(&roi, &card_rotation)?;

                        let mut inner_roi = frame.roi_mut(rotated_rect)?;
                        card_rotation.copy_to(&mut inner_roi)?;
                    }
                }
            } else {
                display_card.pop_front();
                display_start_time = None;
                fade_start_time = None;
                post_fade_start_time = None;
            }
        }

        if display_card.len() == 0 {
            let mut roi = UMat::new(core::UMatUsageFlags::USAGE_DEFAULT);
            let mut inner_roi = frame.roi_mut(card_rect)?;

            inner_roi.copy_to(&mut roi)?;

            let card_rotation = remove_white_corners(&roi, &card_back_img)?;
            card_rotation.copy_to(&mut inner_roi)?;
        }

        out.write(&frame)?;
        if args.timeout.is_some() {
            bar.inc(increment as u64);
        } else {
            bar.inc(1);
        }
    }

    // end progress bar
    bar.finish();
    out.release()?;

    println!("Adding audio...");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&[
        "-i",
        &tmp_path,
        "-i",
        &args.video_file,
        "-c",
        "copy",
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-shortest",
        &output_path,
        "-y",
    ]);

    cmd.output()?;
    println!("Finished!");

    Ok(())
}
