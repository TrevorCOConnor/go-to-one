use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use opencv::{
    core::{self, tempfile, Mat, MatTrait, MatTraitConst, Point, Scalar, Size},
    imgcodecs,
    imgproc::{self, put_text, FONT_HERSHEY_SCRIPT_COMPLEX, FONT_HERSHEY_SIMPLEX, LINE_8, LINE_AA},
    videoio::{
        self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter,
        VideoWriterTrait,
    },
};
use serde::Deserialize;
use std::{borrow::BorrowMut, collections::VecDeque, error, io::stdout, ops::Sub};
use textwrap::Options;

// Card display
const MAX_TRANSPARENCY: f64 = 0.8;
const FADE_IN_DURATION: f64 = 0.75;
const DISPLAY_DURATION: f64 = 6.0;
const EXTENDED_DISPLAY_DURATION: f64 = 12.0;
const FADE_OUT_DURATION: f64 = 0.75;

// Constants
const CARD_WIDTH_RATIO: f64 = 450.0 / 628.0;
const MILLI: f64 = 1_000.0;

// Scoreboard dimensions
const SCOREBOARD_WIDTH_RATIO: f64 = 0.2;
const SCOREBOARD_HEIGHT_BUFFER_RATIO: f64 = 0.02;
const SCOREBOARD_WIDTH_BUFFER_RATIO: f64 = 0.01;

// Fonts
const SCORE_FONT_SCALE: f64 = 7.0;
const SCORE_FONT_STYLE: i32 = FONT_HERSHEY_SCRIPT_COMPLEX;
const SCORE_FONT_WIDTH: i32 = 8;

const HERO_FONT_SCALE: f64 = 2.0;
const HERO_FONT_STYLE: i32 = FONT_HERSHEY_SIMPLEX;
const HERO_FONT_WIDTH: i32 = 4;
const HERO_TEXT_LENGTH: Options = Options::new(20);

// Life
const LIFE_TICK: f64 = 250.0;

// File Constants
const LIFE_DATA_TYPE: &str = "life";
const CARD_DATA_TYPE: &str = "card";

// Logo
const LOGO_FP: &str = "data/Fleshandblood_Medium_500.png";

// Debug FPS
const DEBUG_FPS: f64 = 5.;

// Change the alias to use `Box<dyn error::Error>`.
type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

#[derive(Deserialize, Debug)]
struct DataRow {
    sec: u64,
    milli: f64,
    name: String,
    pitch: Option<u32>,
    player1_life: Option<i32>,
    player2_life: Option<i32>,
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
    IN,
    DISPLAY,
    OUT,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    // Load card db
    let card_db = lib::card::CardDB::init();
    let card_image_db = lib::card::CardImageDB::init();
    let heroes = card_db.heroes();

    // Init user input
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout)?;

    println!("Choose hero 1:");
    let hero1 = lib::commands::enter_card(&heroes).await;
    println!("Choose hero 2:");
    let hero2 = lib::commands::enter_card(&heroes).await;

    // End user input
    disable_raw_mode()?;

    let hero = format!("{}\n vs\n{}", hero1.name, hero2.name);

    // Load game stats
    let mut rows: VecDeque<std::result::Result<DataRow, csv::Error>> = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(args.card_file)
        .expect("Could not load card file")
        .deserialize()
        .collect();

    let output_path = format!("output_videos/{}_output_video.mp4", chrono::Local::now());

    // Create capture
    let mut cap = VideoCapture::from_file(&args.video_file, videoio::CAP_ANY)?;
    let original_width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)?;
    let original_height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)?;
    let original_fps = cap.get(videoio::CAP_PROP_FPS)?;

    let fps = {
        if args.debug {
            DEBUG_FPS
        } else {
            original_fps
        }
    };

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

    // Scoreboard dimensions
    let scoreboard_width = (frame_width * SCOREBOARD_WIDTH_RATIO) as i32;
    let scoreboard_width_buffer = (frame_width * SCOREBOARD_WIDTH_BUFFER_RATIO) as i32;
    let scoreboard_height_buffer = (frame_height * SCOREBOARD_HEIGHT_BUFFER_RATIO) as i32;
    let scoreboard_height = (frame_height as i32) - 5 * scoreboard_height_buffer;

    // Card dimensions
    let card_height = scoreboard_height / 2;
    let card_width = ((card_height as f64) * CARD_WIDTH_RATIO) as i32;

    let increment = original_fps.recip() * MILLI;

    // Generate output video
    let mut out = VideoWriter::new(
        output_path,
        VideoWriter::fourcc('m', 'p', '4', 'v').unwrap(),
        fps,
        Size::new(frame_width as i32, frame_height as i32),
        true,
    )?;

    // Set init vars
    let mut display_start_time = None;
    let mut fade_start_time: Option<TimeTick> = None;
    let mut time_tick = TimeTick::new();
    let mut display_card: VecDeque<DataRow> = VecDeque::new();
    let image_file = tempfile(".png").unwrap();

    // Set default life
    let mut player1_life: i32 = 40;
    let mut player2_life: i32 = 40;

    // Check for init lifes row
    let first_row = &rows
        .front()
        .expect("Empty data file")
        .as_ref()
        .expect("Broken data found at first row");
    if first_row.update_type == LIFE_DATA_TYPE {
        player1_life = first_row.player1_life.unwrap_or(40);
        player2_life = first_row.player2_life.unwrap_or(40);
    }

    // Track what the players lives should be so we can tick them down
    let mut player1_display_life: i32 = player1_life;
    let mut player2_display_life: i32 = player2_life;

    let mut life_ticker = 0;
    let life_ticker_mod = (LIFE_TICK / increment) as u32;

    let mut debug_tracker = 0_u32;
    let debug_skip_count = (original_fps / fps) as u32;

    // LOOP HERE
    loop {
        if let Some(sec) = args.timeout {
            if time_tick.sec > sec {
                break;
            }
        }

        let mut frame = Mat::default();
        time_tick.increment_milli(increment);

        // Increment life ticker
        life_ticker += 1;
        life_ticker = life_ticker % life_ticker_mod;

        // Grab frame
        if !cap.read(&mut frame).unwrap_or(false) {
            break;
        }

        // Speed up debug runs
        if args.debug {
            // Skip frame
            if debug_tracker < debug_skip_count {
                debug_tracker += 1;
                continue;
            } else {
                debug_tracker = 0;
            }
        }

        // Draw Scoreboard
        let _ = imgproc::rectangle(
            &mut frame,
            core::Rect::new(0, 0, scoreboard_width, frame_height as i32),
            Scalar::new(0.0, 0.0, 0.0, 0.0),
            -1, // Thickness of -1 fills the rectangle completely
            LINE_8,
            0,
        );

        // Update life totals
        if life_ticker == 0 {
            if player1_display_life != player1_life {
                player1_display_life += (player1_life - player1_display_life).signum();
            }
            if player2_display_life != player2_life {
                player2_display_life += (player2_life - player2_display_life).signum();
            }
        }

        // Player1 Life
        put_text(
            &mut frame,
            &player1_display_life.to_string(),
            Point::new(
                scoreboard_width_buffer,
                scoreboard_height / 6 - scoreboard_height_buffer,
            ),
            SCORE_FONT_STYLE,
            SCORE_FONT_SCALE,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            SCORE_FONT_WIDTH,
            LINE_AA,
            false,
        )?;
        // Player2 Life
        put_text(
            &mut frame,
            &player2_display_life.to_string(),
            Point::new(
                scoreboard_width / 2 + scoreboard_width_buffer,
                scoreboard_height / 6 - scoreboard_height_buffer,
            ),
            SCORE_FONT_STYLE,
            SCORE_FONT_SCALE,
            Scalar::new(255.0, 255.0, 255.0, 0.0),
            SCORE_FONT_WIDTH,
            LINE_AA,
            false,
        )?;
        // Draw Line between player lives
        let _ = imgproc::line(
            &mut frame,
            Point::new(scoreboard_width / 2, scoreboard_height_buffer),
            Point::new(
                scoreboard_width / 2,
                scoreboard_height_buffer + scoreboard_height / 6,
            ),
            Scalar::new(255.0, 255.0, 255.0, 255.0),
            SCORE_FONT_WIDTH,
            LINE_AA,
            0,
        );

        // GoToOne Logo
        let mut logo_image = imgcodecs::imread(&LOGO_FP, imgcodecs::IMREAD_COLOR).unwrap();
        opencv::imgproc::resize(
            &logo_image.clone(),
            &mut logo_image,
            Size::new(
                scoreboard_width - 2 * scoreboard_width_buffer,
                scoreboard_height / 6,
            ),
            0.0,
            0.0,
            opencv::imgproc::INTER_LINEAR,
        )?;
        let mut logo_roi = frame.roi_mut(core::Rect::new(
            scoreboard_width_buffer,
            2 * scoreboard_height_buffer + (scoreboard_height / 6),
            scoreboard_width - 2 * scoreboard_width_buffer,
            scoreboard_height / 6,
        ))?;
        let _ = logo_image.copy_to(logo_roi.borrow_mut());

        // Hero names
        let wrapped_hero = textwrap::wrap(&hero, HERO_TEXT_LENGTH);
        for (e, line) in wrapped_hero.iter().enumerate() {
            let e = e as i32;
            put_text(
                &mut frame,
                line,
                Point::new(
                    scoreboard_width_buffer,
                    2 * (scoreboard_height / 6)
                        + 3 * (scoreboard_height_buffer)
                        + ((e + 1) * (frame_height as i32 / 30)),
                ),
                HERO_FONT_STYLE,
                HERO_FONT_SCALE,
                Scalar::new(255.0, 255.0, 255.0, 0.0),
                HERO_FONT_WIDTH,
                LINE_AA,
                false,
            )?;
        }

        // Rotate frame if necessary
        if rotate {
            let mut rotated_frame = Mat::default();
            core::transpose(&frame, &mut rotated_frame)?;
            opencv::core::rotate(
                &frame,
                &mut rotated_frame,
                opencv::core::ROTATE_90_CLOCKWISE,
            )
            .unwrap();
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
                } else {
                    if let Some(life) = row.player1_life {
                        player1_life = life;
                    }
                    if let Some(life) = row.player2_life {
                        player2_life = life;
                    }
                }
            }
        }

        // Add start time and card image
        if let (Some(card), None) = (display_card.front(), display_start_time) {
            display_start_time = Some(time_tick.clone());
            println!("{}", card.name);
            card_image_db
                .load_card_image(&card.name, &card.pitch, &image_file)
                .await;
        }

        // Display card
        if let (Some(_), Some(start_time)) = (&display_card.front(), &display_start_time) {
            let elapsed_time = (time_tick - *start_time).as_f64();
            if elapsed_time <= EXTENDED_DISPLAY_DURATION
                && fade_start_time.is_none_or(|v| (time_tick - v).as_f64() < FADE_OUT_DURATION)
            {
                let fade_stage = {
                    // Fade in
                    if elapsed_time < FADE_IN_DURATION {
                        FadeStage::IN
                    // Minimum Display time
                    } else if elapsed_time < DISPLAY_DURATION - FADE_OUT_DURATION {
                        FadeStage::DISPLAY
                    // Extended display
                    } else if elapsed_time < EXTENDED_DISPLAY_DURATION - FADE_OUT_DURATION
                        && display_card.len() == 1
                    {
                        FadeStage::DISPLAY
                    // Fade out
                    } else {
                        FadeStage::OUT
                    }
                };

                // Start fade out timer if not started yet
                if fade_stage == FadeStage::OUT && fade_start_time.is_none() {
                    let _ = fade_start_time.insert(time_tick.clone());
                }

                let mut card_image =
                    imgcodecs::imread(&image_file, imgcodecs::IMREAD_COLOR).unwrap();
                opencv::imgproc::resize(
                    &card_image.clone(),
                    &mut card_image,
                    Size::new(card_width, card_height),
                    0.0,
                    0.0,
                    opencv::imgproc::INTER_LINEAR,
                )?;

                let y_offset = 4 * scoreboard_height_buffer + 3 * (scoreboard_height / 6);
                let new_frame = frame.clone();

                let roi = new_frame.roi(core::Rect::new(
                    scoreboard_width_buffer,
                    y_offset,
                    card_width,
                    card_height,
                ))?;

                let mut inner_roi = frame.roi_mut(core::Rect::new(
                    scoreboard_width_buffer,
                    y_offset,
                    card_width,
                    card_height,
                ))?;

                let alpha = match fade_stage {
                    FadeStage::IN => MAX_TRANSPARENCY * (elapsed_time / FADE_IN_DURATION),
                    FadeStage::DISPLAY => MAX_TRANSPARENCY,
                    FadeStage::OUT => {
                        MAX_TRANSPARENCY
                            * (1.0
                                - ((time_tick - fade_start_time.unwrap()).as_f64()
                                    / FADE_OUT_DURATION))
                    }
                };

                core::add_weighted(
                    &roi,
                    1.0 - alpha,
                    &card_image,
                    alpha,
                    0.0,
                    &mut inner_roi,
                    -1,
                )?;
            } else {
                display_card.pop_front();
                display_start_time = None;
                fade_start_time = None;
            }
        }

        out.write(&frame)?;
    }

    Ok(())
}
