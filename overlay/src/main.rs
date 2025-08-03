use clap::Parser;
use indicatif::ProgressBar;
use log::{debug};

use lib::{
    card::CardImageDB,
    fade::{convert_alpha_to_white, remove_color, remove_white_corners},
    image::{load_image, load_image_unchanged, FullArtHeroManager},
    intro::{generate_intro, VideoCapLooper, VideoCapLooperAdj, INTRO_TIME},
    life_tracker::LifeTracker,
    movement::{
        place_umat, relocate_umat, resize_umat, safe_scale, straight_line, MoveFunction,
        Reparameterization,
    },
    relative_roi::{center_offset, HorizontalPartition, RelativeRoi, VerticalPartition},
    rotate::{rotate_image, REMOVAL_COLOR},
    text::{center_text_at_rect, center_text_at_rel},
};
use opencv::{
    core::{self, flip, set_use_opencl, Point, Rect, Scalar, Size, UMat, UMatTrait, UMatTraitConst},
    imgproc::{
        self, cvt_color_def, COLOR_RGBA2RGB, FONT_HERSHEY_SCRIPT_COMPLEX, FONT_HERSHEY_SIMPLEX,
        LINE_8,
    },
    videoio::{
        self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter,
        VideoWriterTrait, CAP_PROP_FRAME_COUNT, CAP_PROP_POS_FRAMES,
    },
};
use serde::Deserialize;
use std::{borrow::BorrowMut, collections::VecDeque, error, ops::Sub, process::Command};
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
const WHITE: Scalar = Scalar::new(255.0, 255.0, 255.0, 0.0);

// Background
const BACKGROUND_ANIM_FILE: &'static str = "data/smaller_hexagon.mp4";

// Frame dimensions
const FRAME_HEIGHT_RATIO: f64 = 1.0 - (1.0 / 64.0);

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

enum CardDisplayPhase {
    CardBackRotateOut,
    CardFrontRotateIn,
    Display,
    Extended,
    CardFrontRotateOut,
    CardBackRotateIn,
    Sleep,
    ZoomIn,
    ZoomDisplay,
    ZoomOut,
    PostZoom,
}

struct CardDisplayManager {
    card_rect: Rect,
    card_db: lib::card::CardImageDB,
    card_back: UMat,
    display_card: Option<UMat>,
    phase: CardDisplayPhase,
    queue: VecDeque<DataRow>,
    timer: TimeTick,
    zoom: bool,
}

impl CardDisplayManager {
    fn queue_zoom(&mut self) {
        self.queue.push_back(DataRow {
            update_type: ZOOM.to_owned(),
            ..Default::default()
        });
    }

    fn add_card_to_queue(&mut self, card: DataRow) {
        self.queue.push_back(card);
    }

    fn new(card_rect: &Rect, card_back: &UMat, time_tick: &TimeTick) -> Self {
        let card_db = CardImageDB::init();
        Self {
            card_rect: card_rect.clone(),
            card_db,
            card_back: card_back.clone(),
            display_card: None,
            phase: CardDisplayPhase::Sleep,
            queue: VecDeque::new(),
            timer: time_tick.clone(),
            zoom: false,
        }
    }

    fn tick(&mut self, time_tick: TimeTick, frame: &mut UMat, frame_rect: &Rect) -> Result<()> {
        let elapsed_time = (time_tick - self.timer).as_f64();

        // Check for zoom
        if self.queue.len() > 0 {
            if self.queue.front().as_ref().unwrap().update_type == ZOOM {
                self.queue.pop_front();
                // ignore zooms not attached to a card
                if self.display_card.is_some() {
                    self.zoom = true;
                }
            }
        }
        match self.phase {
            CardDisplayPhase::CardBackRotateOut => {
                if elapsed_time >= ROTATE_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::CardFrontRotateIn;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let t = elapsed_time / ROTATE_TIME;
                    let rotated = rotate_image(&self.card_back, t as f32, true)?;
                    let rotated_rect = core::Rect::new(
                        self.card_rect.x,
                        self.card_rect.y
                            - center_offset(self.card_rect.height, rotated.size()?.height),
                        rotated.size()?.width,
                        rotated.size()?.height,
                    );

                    let roi = &frame.roi(rotated_rect)?;

                    let card_rotation =
                        remove_color(&roi, &rotated, &REMOVAL_COLOR)?;
                    let mut inner_roi = frame.roi_mut(rotated_rect)?;
                    card_rotation.copy_to(&mut inner_roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::CardFrontRotateIn => {
                if elapsed_time >= ROTATE_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::Display;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let t = elapsed_time / ROTATE_TIME;
                    let display_card = self.display_card.as_ref().unwrap();
                    let green = UMat::new_size_with_default_def(
                        display_card.size()?,
                        display_card.typ(),
                        REMOVAL_COLOR,
                    )?;
                    let card = remove_white_corners(&green, &display_card)?;

                    let rotated = rotate_image(&card, t as f32, false)?;
                    let rotated_rect = core::Rect::new(
                        self.card_rect.x,
                        self.card_rect.y - (rotated.rows() - self.card_rect.height).div_euclid(2),
                        rotated.cols(),
                        rotated.rows(),
                    );

                    let mut roi = frame.roi_mut(rotated_rect)?;
                    let card_rotation =
                        remove_color(&roi, &rotated, &REMOVAL_COLOR)?;
                    card_rotation.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::Display => {
                if self.zoom {
                    self.timer = time_tick.clone();
                    self.zoom = false;
                    self.phase = CardDisplayPhase::ZoomIn;
                    self.tick(time_tick, frame, frame_rect)
                } else if elapsed_time >= DISPLAY_DURATION {
                    if self.queue.len() == 0 {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::Extended;
                        self.tick(time_tick, frame, frame_rect)
                    } else {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::CardFrontRotateOut;
                        self.tick(time_tick, frame, frame_rect)
                    }
                } else {
                    let display_card = self.display_card.as_ref().unwrap();
                    let mut roi = frame.roi_mut(self.card_rect)?;

                    let card = remove_white_corners(&roi, &display_card)?;
                    card.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::CardFrontRotateOut => {
                if elapsed_time >= ROTATE_TIME {
                    if self.queue.len() == 0 {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::CardBackRotateIn;
                        self.tick(time_tick, frame, frame_rect)
                    } else {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::CardFrontRotateIn;
                        let card = self.queue.pop_front().unwrap();
                        self.load_card_image(&card)?;
                        self.tick(time_tick, frame, frame_rect)
                    }
                } else {
                    let t = elapsed_time / FADE_OUT_DURATION;
                    let display_card = self.display_card.as_ref().unwrap();
                    let green = UMat::new_size_with_default_def(
                        display_card.size()?,
                        display_card.typ(),
                        REMOVAL_COLOR
                    )?;
                    let card = remove_white_corners(&green, &display_card)?;
                    let rotated = rotate_image(&card, t as f32, true)?;
                    let rotated_rect = core::Rect::new(
                        self.card_rect.x,
                        self.card_rect.y - (rotated.rows() - self.card_rect.height).div_euclid(2),
                        rotated.cols(),
                        rotated.rows(),
                    );

                    let mut roi = frame.roi_mut(rotated_rect)?;

                    let card_rotation =
                        remove_color(&roi, &rotated, &REMOVAL_COLOR)?;
                    let card_rotation = remove_white_corners(&roi, &card_rotation)?;

                    card_rotation.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::CardBackRotateIn => {
                if elapsed_time >= ROTATE_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::Sleep;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let t = elapsed_time / ROTATE_TIME;
                    let green = UMat::new_size_with_default_def(
                        self.card_back.size()?,
                        self.card_back.typ(),
                        REMOVAL_COLOR
                    )?;
                    let card = remove_white_corners(&green, &self.card_back)?;

                    let rotated = rotate_image(&card, t as f32, false)?;
                    let rotated_rect = core::Rect::new(
                        self.card_rect.x,
                        self.card_rect.y - (rotated.rows() - self.card_rect.height).div_euclid(2),
                        rotated.cols(),
                        rotated.rows(),
                    );

                    let mut roi = frame.roi_mut(rotated_rect)?;
                    let card_rotation =
                        remove_color(&roi, &rotated, &REMOVAL_COLOR)?;
                    card_rotation.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::ZoomIn => {
                if elapsed_time >= ZOOM_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::ZoomDisplay;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let card = self.display_card.as_ref().unwrap();
                    let percentage = elapsed_time / ZOOM_TIME;
                    let scale_percentage = Reparameterization::SCurve.apply(percentage);

                    let goal_location = Point::new(
                        frame_rect.x + center_offset(self.card_rect.width, frame_rect.width),
                        frame_rect.y + center_offset(self.card_rect.height, frame_rect.height),
                    );

                    let relocation = relocate_umat(
                        &Point::new(self.card_rect.x, self.card_rect.y),
                        &goal_location,
                        &card,
                        frame,
                        percentage,
                        MoveFunction::SlowFastSlowCurve,
                    )?;
                    let resized = safe_scale(
                        &relocation,
                        &frame.size()?,
                        straight_line(1.0, 1.5, scale_percentage),
                    )?;
                    let sized_img = resize_umat(card, &resized.size())?;
                    let roi = frame.roi(resized)?;
                    let sized_img = remove_white_corners(&roi, &sized_img)?;
                    place_umat(&sized_img, frame, resized)?;
                    Ok(())
                }
            }
            CardDisplayPhase::ZoomDisplay => {
                if elapsed_time >= ZOOM_DISPLAY {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::ZoomOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let card = self.display_card.as_ref().unwrap();
                    let scale_percentage = Reparameterization::SCurve.apply(1.0);

                    let goal_location = Point::new(
                        frame_rect.x + center_offset(self.card_rect.width, frame_rect.width),
                        frame_rect.y + center_offset(self.card_rect.height, frame_rect.height),
                    );

                    let relocation = relocate_umat(
                        &Point::new(self.card_rect.x, self.card_rect.y),
                        &goal_location,
                        &card,
                        frame,
                        1.0,
                        MoveFunction::SlowFastSlowCurve,
                    )?;
                    let resized = safe_scale(
                        &relocation,
                        &frame.size()?,
                        straight_line(1.0, 1.5, scale_percentage),
                    )?;
                    let sized_img = resize_umat(card, &resized.size())?;
                    let roi = frame.roi(resized)?;
                    let sized_img = remove_white_corners(&roi, &sized_img)?;
                    place_umat(&sized_img, frame, resized)?;
                    Ok(())
                }
            }
            CardDisplayPhase::ZoomOut => {
                if elapsed_time >= ZOOM_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::PostZoom;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let card = self.display_card.as_ref().unwrap();
                    let percentage = 1.0 - (elapsed_time / ZOOM_TIME);
                    let scale_percentage = Reparameterization::SCurve.apply(percentage);

                    let goal_location = Point::new(
                        frame_rect.x + center_offset(self.card_rect.width, frame_rect.width),
                        frame_rect.y + center_offset(self.card_rect.height, frame_rect.height),
                    );

                    let relocation = relocate_umat(
                        &Point::new(self.card_rect.x, self.card_rect.y),
                        &goal_location,
                        &card,
                        frame,
                        percentage,
                        MoveFunction::SlowFastSlowCurve,
                    )?;
                    let resized = safe_scale(
                        &relocation,
                        &frame.size()?,
                        straight_line(1.0, 1.5, scale_percentage),
                    )?;
                    let sized_img = resize_umat(card, &resized.size())?;
                    let roi = frame.roi(resized)?;
                    let sized_img = remove_white_corners(&roi, &sized_img)?;
                    place_umat(&sized_img, frame, resized)?;
                    Ok(())
                }
            }
            CardDisplayPhase::PostZoom => {
                if elapsed_time >= POST_ZOOM_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::CardFrontRotateOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let display_card = self.display_card.as_ref().unwrap();
                    let mut roi = frame.roi_mut(self.card_rect)?;

                    let card = remove_white_corners(&roi, &display_card)?;
                    card.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::Extended => {
                if elapsed_time >= EXTENDED_DISPLAY_DURATION || self.queue.len() > 0 {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::CardFrontRotateOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let display_card = self.display_card.as_ref().unwrap();
                    let mut roi = frame.roi_mut(self.card_rect)?;

                    let card = remove_white_corners(&roi, &display_card)?;
                    card.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::Sleep => {
                if self.queue.len() > 0 {
                    self.timer = time_tick.clone();
                    let card = self.queue.pop_front().unwrap();
                    self.load_card_image(&card)?;

                    self.phase = CardDisplayPhase::CardBackRotateOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let roi = frame.roi(self.card_rect)?;
                    let card = remove_color(&roi, &self.card_back, &REMOVAL_COLOR)?;
                    place_umat(&card, frame, self.card_rect)?;
                    Ok(())
                }
            }
        }
    }

    fn load_card_image(&mut self, display_card: &DataRow) -> Result<()> {
        let mut img = self
            .card_db
            .load_card_image(&display_card.name, &display_card.pitch);
        if img.cols() > img.rows() {
            let mut rotated_card_image = UMat::new_def();
            opencv::core::rotate(
                &img,
                &mut rotated_card_image,
                opencv::core::ROTATE_90_CLOCKWISE,
            )?;
            img = rotated_card_image
        }
        opencv::imgproc::resize(
            &img.clone(),
            &mut img,
            self.card_rect.size(),
            0.0,
            0.0,
            opencv::imgproc::INTER_LINEAR,
        )?;
        self.display_card.replace(img);
        Ok(())
    }
}

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

    #[arg(long, action)]
    skip_intro: bool,

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

fn main() -> Result<()> {
    let args = Cli::parse();
    set_use_opencl(true)?;

    let mut platforms = opencv::core::Vector::new();
    opencv::core::get_platfoms_info(&mut platforms)?;

    // Check debug
    if args.debug {
        println!("debugging");
        simple_logging::log_to_file("log.txt", log::LevelFilter::Debug).unwrap(); 
    }

    // Load game stats
    let mut rows: VecDeque<std::result::Result<DataRow, csv::Error>> = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(args.card_file)
        .expect("Could not load card file")
        .deserialize()
        .collect();

    // Get player names
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
        4.0 / 9.0,
        Some(WIDTH_BUFFER_RATIO),
        Some(2.0 * HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Top),
    )?;
    let card_rel_roi = RelativeRoi::build_as_partition(
        0.0,
        4.0 / 9.0,
        SIDE_PANEL_WIDTH_RATIO,
        5.0 / 9.0,
        Some(WIDTH_BUFFER_RATIO),
        Some(2.0 * HEIGHT_BUFFER_RATIO),
        Some(HorizontalPartition::Left),
        Some(VerticalPartition::Bottom),
    )?;

    // Get hero images
    let full_art_manager = FullArtHeroManager::new();
    let hero1_animation_fp = full_art_manager.get_cropped_hero_art_animation_fp(&hero1_stats.name)?;
    let hero2_animation_fp = full_art_manager.get_cropped_hero_art_animation_fp(&hero2_stats.name)?;

    let mut hero1_animation = VideoCapLooperAdj::build(&hero1_animation_fp)?;
    let mut hero2_animation = VideoCapLooperAdj::build(&hero2_animation_fp)?;

    // Load card back
    let card_back_img = load_image(&CARD_BACK_FP)?;
    let green_background =
        UMat::new_size_with_default_def(card_back_img.size()?, card_back_img.typ(), REMOVAL_COLOR)?;
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

    if !args.skip_intro {
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
    }

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
        if args.timeout.is_some() {
            ProgressBar::new(((args.timeout.unwrap() + 1) as f64 * MILLI) as u64)
        } else {
            ProgressBar::new(cap.get(CAP_PROP_FRAME_COUNT).unwrap() as u64)
        }
    };

    let mut card_display_manager = CardDisplayManager::new(&card_rect, &card_back_img, &time_tick);

    // Cut beginning of video where intro would be
    if !args.skip_intro {
        let intro_frames = INTRO_TIME * fps;
        cap.set(CAP_PROP_POS_FRAMES, intro_frames)?;
        time_tick.increment_milli(increment * intro_frames);
    }

    // LOOP HERE
    println!("overlaying video...");
    loop {
        // Check timeout
        if let Some(sec) = args.timeout {
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

        // Crop frame
        let crop_left =
            ((args.crop_left.unwrap_or(0.0) / 100.0) * frame.size()?.width as f64) as i32;
        let crop_right =
            ((args.crop_right.unwrap_or(0.0) / 100.0) * frame.size()?.width as f64) as i32;
        let crop_top =
            ((args.crop_top.unwrap_or(0.0) / 100.0) * frame.size()?.height as f64) as i32;
        let crop_bottom =
            ((args.crop_bottom.unwrap_or(0.0) / 100.0) * frame.size()?.height as f64) as i32;

        let crop_roi = frame.roi(core::Rect::new(
            crop_left,
            crop_top,
            frame.size()?.width - (crop_left + crop_right),
            ((frame.size()?.height - (crop_top + crop_bottom)) as f64 * FRAME_HEIGHT_RATIO) as i32,
        ))?;
        let mut innerframe = UMat::new_def();
        crop_roi.copy_to(&mut innerframe)?;

        // Reframe
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
        let now = std::time::Instant::now();
        let hero1_image = hero1_animation.read()?;
        let elapsed = now.elapsed();
        debug!("Read hero: {:?}", elapsed);

        // let now = std::time::Instant::now();
        // let hero1_image = FullArtHeroManager::crop_hero_img(&hero1_image)?;
        // let elapsed = now.elapsed();
        // debug!("Crop hero: {:?}", elapsed);

        let now = std::time::Instant::now();
        let hero1_rect = hero1_rel_roi.generate_roi(&frame_size, &hero1_image);
        let mut hero1_image = hero1_rel_roi.resize(&frame_size, &hero1_image)?;
        let elapsed = now.elapsed();
        debug!("Resize hero: {:?}", elapsed);

        let now = std::time::Instant::now();
        flip(&hero1_image.clone(), &mut hero1_image, 1)?;
        let elapsed = now.elapsed();
        debug!("Flip hero: {:?}", elapsed);

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
        // let hero2_image = FullArtHeroManager::crop_hero_img(&hero2_image)?;
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

        // Player details
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
