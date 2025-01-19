use clap::Parser;
use opencv::{
    core::{self, tempfile, Mat, MatTrait, MatTraitConst, Point, Scalar, Size},
    imgcodecs,
    // imgproc::{put_text, FONT_HERSHEY_COMPLEX, LINE_AA},
    videoio::{
        self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter,
        VideoWriterTrait,
    },
    Error,
};
use pod::CardDB;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::VecDeque, fs::File, io::Read, ops::Sub};

// Defaults
const DEFAULT_MAX_TRANSPARENCY: f64 = 0.8;
const DEFAULT_FADE_IN_DURATION: f64 = 0.75;
const DEFAULT_DISPLAY_DURATION: f64 = 6.0;
const DEFAULT_FADE_OUT_DURATION: f64 = 0.75;

const CARD_WIDTH: i32 = 450;
const CARD_HEIGHT: i32 = 628;

const MILLI: f64 = 1_000.0;

struct Config {
    max_transparency: f64,
    fade_in_duration: f64,
    display_duration: f64,
    fade_out_duration: f64,
}

impl Config {
    fn load_from_file(fp: String) -> Self {
        let mut data = String::new();

        File::open(fp)
            .expect("Could not load find config file")
            .read_to_string(&mut data)
            .expect("Could not load config file");
        let json: Value = serde_json::from_str(&data).expect("Could not read config file.");

        // Probably figure out a better way to do this later
        // Built in default uses a function, which seems just as tedious
        Config {
            max_transparency: json
                .get("max_transparency")
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_MAX_TRANSPARENCY),
            fade_in_duration: json
                .get("fade_in_duration")
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_FADE_IN_DURATION),
            display_duration: json
                .get("display_duration")
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_DISPLAY_DURATION),
            fade_out_duration: json
                .get("fade_out_duration")
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_FADE_OUT_DURATION),
        }
    }
}

#[derive(Deserialize)]
struct CardRow {
    sec: u64,
    milli: f64,
    uuid: String,
    name: String,
    pitch: Option<u32>,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    video_file: String,

    #[arg(short, long)]
    card_file: String,

    #[arg(long)]
    cfg: String,

    #[arg(short, long)]
    timeout: Option<u64>,
}

#[derive(Clone, Copy)]
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

#[derive(Debug)]
enum FadeStage {
    IN,
    DISPLAY,
    OUT,
}

fn main() -> Result<(), Error> {
    let args = Cli::parse();

    let cfg = Config::load_from_file(args.cfg);

    let mut rows: VecDeque<Result<CardRow, csv::Error>> = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(args.card_file)
        .expect("Could not load card file")
        .deserialize()
        .collect();

    let output_path = "output_video.mp4";

    let mut cap = VideoCapture::from_file(&args.video_file, videoio::CAP_ANY)?;
    let frame_width = cap.get(videoio::CAP_PROP_FRAME_WIDTH)?;
    let frame_height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;

    let increment = fps.recip() * MILLI;

    let mut out = VideoWriter::new(
        output_path,
        VideoWriter::fourcc('m', 'p', '4', 'v').unwrap(),
        fps,
        Size::new(frame_width as i32, frame_height as i32),
        true,
    )?;

    let card_db = CardDB::init();

    let mut display_start_time = None;
    let mut time_tick = TimeTick::new();
    let mut display_card: VecDeque<CardRow> = VecDeque::new();
    let image_file = tempfile(".png").unwrap();

    loop {
        if let Some(sec) = args.timeout {
            if time_tick.sec > sec {
                break;
            }
        }

        let mut frame = Mat::default();
        time_tick.increment_milli(increment);

        // Grab frame
        if !cap.read(&mut frame).unwrap_or(false) {
            break;
        }

        // Add card to queue
        if let Some(row) = rows.front() {
            let row = row.as_ref().expect("Invalid card data");
            let time = TimeTick::build(row.sec, row.milli);
            // Card time just passed
            if time <= time_tick {
                display_card.push_back(rows.pop_front().unwrap().unwrap());
            }
        }

        // Add start time and card image
        if let (Some(card), None) = (display_card.front(), display_start_time) {
            display_start_time = Some(time_tick.clone());
            println!("{}", card.name);
            card_db.load_card_image(&card.uuid, &image_file);
        }

        // Display card
        if let (Some(_), Some(start_time)) = (&display_card.front(), &display_start_time) {
            if (time_tick - *start_time).as_f64() <= cfg.display_duration {
                let elapsed_time = (time_tick - *start_time).as_f64();
                let fade_stage = {
                    if elapsed_time < cfg.fade_in_duration {
                        FadeStage::IN
                    } else if elapsed_time < cfg.display_duration - cfg.fade_out_duration {
                        FadeStage::DISPLAY
                    } else {
                        FadeStage::OUT
                    }
                };

                let mut card_image =
                    imgcodecs::imread(&image_file, imgcodecs::IMREAD_COLOR).unwrap();
                opencv::imgproc::resize(
                    &card_image.clone(),
                    &mut card_image,
                    Size::new(CARD_WIDTH, CARD_HEIGHT),
                    0.0,
                    0.0,
                    opencv::imgproc::INTER_LINEAR,
                )?;
                let card_size = card_image.size()?;

                let x_offset = (frame_width as i32) - card_size.width - 20;
                let y_offset = 20;
                let new_frame = frame.clone();
                let roi = new_frame.roi(core::Rect::new(
                    x_offset,
                    y_offset,
                    card_size.width,
                    card_size.height,
                ))?;

                let mut inner_roi = frame.roi_mut(core::Rect::new(
                    x_offset,
                    y_offset,
                    card_size.width,
                    card_size.height,
                ))?;

                let alpha = match fade_stage {
                    FadeStage::IN => cfg.max_transparency * (elapsed_time / cfg.fade_in_duration),
                    FadeStage::DISPLAY => cfg.max_transparency,
                    FadeStage::OUT => {
                        cfg.max_transparency
                            * ((cfg.display_duration - elapsed_time) / cfg.fade_out_duration)
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

                // put_text(
                //     &mut frame,
                //     name,
                //     Point::new(x_offset, y_offset - 10),
                //     FONT_HERSHEY_COMPLEX,
                //     0.5,
                //     Scalar::new(255.0, 255.0, 255.0, 0.0),
                //     2,
                //     LINE_AA,
                //     false,
                // )?;
            } else {
                display_card.pop_front();
                display_start_time = None;
            }
        }

        out.write(&frame)?;
    }

    Ok(())
}
