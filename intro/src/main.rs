use std::{
    borrow::BorrowMut,
    collections::{HashMap, VecDeque},
    f64::consts::E,
    fs::File,
    io::Write,
};

use clap::Parser;
use opencv::{
    boxed_ref::BoxedRefMut,
    core::{
        add_weighted, tempfile, Mat, MatTrait, MatTraitConst, Rect, Scalar, Size, UMat, UMatTrait,
        UMatTraitConst, Vec3b, CV_8UC3,
    },
    imgcodecs,
    imgproc::resize,
    videoio::{
        self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriter,
        VideoWriterTrait, CAP_PROP_FRAME_HEIGHT, CAP_PROP_FRAME_WIDTH,
    },
};

// Duration constants
const IMAGE_DURATION: f64 = 5.0;
const HERO_DURATION: f64 = 8.0;
const FADE_IN_DURATION: f64 = 2.0;
const FADE_OUT_DURATION: f64 = 2.0;

// File constants
const HERO_FILE: &'static str = "data/hero_link.json";
const IMAGE_FILE: &'static str = "data/asset_link.json";
const EXUDE_FILE: &'static str = "data/exude_full_1239a0sf123.original.jpg";
const FIRE_OVERLAY: &'static str = "data/199621-910995780.mp4";

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    video_file: String,

    #[arg(short, long)]
    output_file: String,

    #[arg(long)]
    hero1: String,

    #[arg(long)]
    hero2: String,

    #[arg(long)]
    images: Vec<String>,
}

fn fade_function(percent: f64) -> f64 {
    let scalar = (E.powi(2) - 1.0).recip();
    scalar * (E.powf(2.0 * percent) - 1.0)
}

fn load_map(fp: &str) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    let file = File::open(&fp).expect(&format!("Could not find {}", fp));
    let hero_json: serde_json::Value =
        serde_json::from_reader(file).expect("Image map file improperly formatted.");

    for (key, value) in hero_json.as_object().unwrap().iter() {
        map.insert(
            key.to_owned(),
            value
                .as_str()
                .expect("Value is not valid string")
                .to_string(),
        );
    }

    map
}

fn load_image(url: &str) -> Result<UMat, Box<dyn std::error::Error>> {
    // create tmp file to load image
    let tmp_file = tempfile(".png")?;
    let mut file = std::fs::File::create(&tmp_file).unwrap();

    // load image
    let bytes = reqwest::blocking::get(url).unwrap().bytes().unwrap();
    let _ = file.write_all(&bytes);
    let _display = imgcodecs::imread(&tmp_file, imgcodecs::IMREAD_COLOR).unwrap();

    // transfer image to umat
    let mut display = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    _display.copy_to(&mut display)?;

    Ok(display)
}

fn resize_preserve_ratio(img: &UMat, size: &Size) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut frame = UMat::new_rows_cols_with_default(
        size.height,
        size.width,
        CV_8UC3,
        Scalar::new(0., 0., 0., 0.),
        opencv::core::UMatUsageFlags::USAGE_DEFAULT,
    )?;

    let mut img_copy = img.clone();

    let ratio = img.rows() as f64 / img.cols() as f64;
    let adjusted_width = ((size.height as f64) * ratio.recip()) as i32;
    let adjusted_height = ((size.width as f64) * ratio) as i32;

    let new_size = {
        if adjusted_width <= size.width {
            Size::new(adjusted_width, size.height)
        } else {
            Size::new(size.width, adjusted_height)
        }
    };

    let centered_roi = Rect::new(
        (size.width - new_size.width).div_euclid(2),
        (size.height - new_size.height).div_euclid(2),
        new_size.width,
        new_size.height,
    );
    let mut roi = frame
        .roi_mut(centered_roi)
        .map_err(|_| "Centered error failed")?;

    resize(&img.clone(), &mut img_copy, new_size, 0., 0., 0)?;
    img_copy.copy_to(&mut roi.borrow_mut())?;

    Ok(frame)
}

fn display_image(
    writer: &mut VideoWriter,
    img_frame: &UMat,
    fade_in_count: u32,
    display_count: u32,
    fade_out_count: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let black_frame = UMat::new_rows_cols_with_default(
        img_frame.rows(),
        img_frame.cols(),
        CV_8UC3,
        Scalar::new(0., 0., 0., 0.),
        opencv::core::UMatUsageFlags::USAGE_DEFAULT,
    )?;
    for i in 0..fade_in_count {
        let mut frame = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        // let alpha = i as f64 / fade_in_count as f64;
        let percent = i as f64 / fade_in_count as f64;
        let alpha = fade_function(percent);
        add_weighted(
            &black_frame,
            1.0 - alpha,
            img_frame,
            alpha,
            0.,
            &mut frame,
            0,
        )?;
        writer.write(&frame)?;
    }
    for _ in 0..display_count {
        writer.write(&img_frame)?;
    }
    for i in 0..fade_out_count {
        let mut frame = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        // let alpha = i as f64 / fade_out_count as f64;
        let percent = i as f64 / fade_in_count as f64;
        let alpha = fade_function(percent);
        add_weighted(
            &img_frame,
            1.0 - alpha,
            &black_frame,
            alpha,
            0.,
            &mut frame,
            0,
        )?;
        writer.write(&frame)?;
    }
    Ok(())
}

fn determine_region_fade_percentage(
    roi: &BoxedRefMut<UMat>,
) -> Result<f64, Box<dyn std::error::Error>> {
    let mean = opencv::core::mean_def(roi)?;
    let avg_rgb = (mean[0] as f64 + mean[1] as f64 + mean[2] as f64) / 3.0;
    // The closer to 0, the darker the pixel
    let fade_percent = avg_rgb / 255.0;
    Ok(fade_percent.powf(1.0 / 3.0))
}

fn overlay_video(
    writer: &mut VideoWriter,
    reader_file: &str,
    img: &Mat,
    display_count: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut video = VideoCapture::from_file(reader_file, videoio::CAP_ANY)?;

    for i in 0..display_count {
        println!("{} / {}", i, display_count);
        let mut overlay = Mat::default();
        // Exit early if video ends
        if !video.read(&mut overlay).unwrap_or(false) {
            break;
        }

        let mut resized = Mat::default();

        resize(
            &overlay,
            &mut resized,
            Size::new(img.cols(), img.rows()),
            0.,
            0.,
            0,
        )?;

        let mut result_mat = img.clone();

        for y in 0..resized.rows() {
            for x in 0..resized.cols() {
                let overlay_pixel = resized.at_2d::<Vec3b>(y, x)?;
                let img_pixel = img.at_2d::<Vec3b>(y, x)?;

                let fade_factor = determine_pixel_fade_percentage(&overlay_pixel);
                let faded_pixel = [
                    (f64::from(overlay_pixel[0]) * fade_factor
                        + f64::from(img_pixel[0]) * (1.0 - fade_factor)) as u8,
                    (f64::from(overlay_pixel[1]) * fade_factor
                        + f64::from(img_pixel[1]) * (1.0 - fade_factor)) as u8,
                    (f64::from(overlay_pixel[2]) * fade_factor
                        + f64::from(img_pixel[2]) * (1.0 - fade_factor)) as u8,
                ];
                result_mat.at_2d_mut::<Vec3b>(y, x)?.0 = faded_pixel;
            }
        }
        let mut end = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        result_mat.copy_to(&mut end)?;
        writer.write(&end)?;
    }

    Ok(())
}

fn overlay_video_sectional(
    writer: &mut VideoWriter,
    reader_file: &str,
    img: &UMat,
    pixels: i32,
    count: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut video = VideoCapture::from_file(reader_file, videoio::CAP_ANY)?;

    let height = img.size()?.height;
    let width = img.size()?.width;
    for i in 0..count {
        println!("{} /{}", i, count);
        let mut overlay = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        // Exit if video ends
        if !video.read(&mut overlay).unwrap_or(false) {
            break;
        }

        resize(
            &overlay.clone(),
            &mut overlay,
            Size::new(img.cols(), img.rows()),
            0.,
            0.,
            0,
        )?;

        for y in 0..height.div_euclid(pixels) {
            for x in 0..width.div_euclid(pixels) {
                // resize video to match image

                let width_size = width - pixels * x;
                let height_size = height - pixels * y;
                let rect = Rect::new(
                    pixels * x,
                    pixels * y,
                    pixels.min(width_size),
                    pixels.min(height_size),
                );
                let origin_video_roi = overlay.roi(rect)?.try_clone()?;
                let mut video_roi = overlay.roi_mut(rect)?;

                let img_roi = img.roi(rect)?;

                let fade_factor = determine_region_fade_percentage(&video_roi)?;
                add_weighted(
                    &origin_video_roi,
                    fade_factor,
                    &img_roi,
                    1.0 - fade_factor,
                    0.,
                    &mut video_roi,
                    0,
                )?;
            }
        }
        writer.write(&overlay)?;
    }

    Ok(())
}

fn overlay_video_sectional_with_fade(
    writer: &mut VideoWriter,
    reader_file: &str,
    img: &UMat,
    pixels: i32,
    fade_in_count: u32,
    display_count: u32,
    fade_out_count: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut video = VideoCapture::from_file(reader_file, videoio::CAP_ANY)?;

    let height = img.size()?.height;
    let width = img.size()?.width;
    let count = fade_in_count + fade_in_count + fade_out_count + display_count;

    for i in 0..count {
        println!("{} /{}", i, count);
        let mut overlay = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        if !video.read(&mut overlay).unwrap_or(false) {
            let mut video = VideoCapture::from_file(reader_file, videoio::CAP_ANY)?;
            video.read(&mut overlay).unwrap_or(false);
        }

        // Calculate fade on image
        let percentage = {
            if i <= fade_in_count {
                0.0
            } else if i <= 2 * fade_in_count {
                (i - fade_in_count) as f64 / fade_in_count as f64
            } else if i < 2 * fade_in_count + display_count {
                1.0
            } else {
                1.0 - ((i - (2 * fade_in_count) - display_count) as f64 / (fade_out_count as f64))
            }
        };
        let mut frame = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        let alpha = fade_function(percentage);
        let black_frame = UMat::new_rows_cols_with_default(
            img.rows(),
            img.cols(),
            CV_8UC3,
            Scalar::new(0., 0., 0., 0.),
            opencv::core::UMatUsageFlags::USAGE_DEFAULT,
        )?;
        add_weighted(&black_frame, 1.0 - alpha, img, alpha, 0., &mut frame, 0)?;

        resize(
            &overlay.clone(),
            &mut overlay,
            Size::new(img.cols(), img.rows()),
            0.,
            0.,
            0,
        )?;

        if i <= fade_in_count {
            let overlay_alpha = i as f64 / fade_in_count as f64;
            add_weighted(
                &black_frame,
                1.0 - overlay_alpha,
                &overlay.clone(),
                overlay_alpha,
                0.,
                &mut overlay,
                0,
            )?;
        }

        for y in 0..height.div_euclid(pixels) {
            for x in 0..width.div_euclid(pixels) {
                // resize video to match image

                let width_size = width - pixels * x;
                let height_size = height - pixels * y;
                let rect = Rect::new(
                    pixels * x,
                    pixels * y,
                    pixels.min(width_size),
                    pixels.min(height_size),
                );
                let origin_video_roi = overlay.roi(rect)?.try_clone()?;
                let mut video_roi = overlay.roi_mut(rect)?;

                let img_roi = frame.roi(rect)?;

                let fade_factor = determine_region_fade_percentage(&video_roi)?;
                add_weighted(
                    &origin_video_roi,
                    fade_factor,
                    &img_roi,
                    1.0 - fade_factor,
                    0.,
                    &mut video_roi,
                    0,
                )?;
            }
        }
        writer.write(&overlay)?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    // Get stats from associated video
    let video = VideoCapture::from_file(&args.video_file, videoio::CAP_ANY)?;
    let width = video.get(CAP_PROP_FRAME_WIDTH)? as i32;
    let height = video.get(CAP_PROP_FRAME_HEIGHT)? as i32;
    let fps = video.get(videoio::CAP_PROP_FPS)?;

    // Start video writer
    let mut out = VideoWriter::new(
        &args.output_file,
        VideoWriter::fourcc('m', 'p', '4', 'v').unwrap(),
        fps,
        Size::new(width, height),
        true,
    )?;

    // Init url maps
    let hero_map = load_map(&HERO_FILE);
    let image_map = load_map(&IMAGE_FILE);

    // Load image urls
    let mut images: VecDeque<&str> = VecDeque::new();
    for val in args.images {
        images.push_back(
            &image_map
                .get(&val)
                .expect(&format!("could not find key {}", &val)),
        );
    }

    // Load hero images
    let mut hero1_img = load_image(
        &hero_map
            .get(&args.hero1)
            .expect(&format!("could not find key {}", &args.hero1)),
    )?;
    let mut hero2_img = load_image(
        &hero_map
            .get(&args.hero2)
            .expect(&format!("could not find key {}", &args.hero1)),
    )?;
    hero1_img = resize_preserve_ratio(&hero1_img, &Size::new(width.div_euclid(2), height))?;
    hero2_img = resize_preserve_ratio(&hero2_img, &Size::new(width.div_euclid(2), height))?;

    // Calculate frame count for each image
    let fade_in_count = (FADE_IN_DURATION * fps) as u32;
    let fade_out_count = (FADE_OUT_DURATION * fps) as u32;
    let image_frame_count = ((IMAGE_DURATION * fps) as u32) - (fade_in_count + fade_out_count);
    let hero_frame_count = ((HERO_DURATION * fps) as u32) - (fade_in_count + fade_out_count);

    for img in images {
        let img_frame = load_image(img)?;
        let img_frame = resize_preserve_ratio(&img_frame, &Size::new(width, height))?;
        display_image(
            &mut out,
            &img_frame,
            fade_in_count,
            image_frame_count,
            fade_out_count,
        )?;
    }

    // Create hero frame
    let mut hero_frame = UMat::new_size_def(Size::new(width, height), CV_8UC3)?;
    let mut left_roi = hero_frame
        .roi_mut(Rect::new(0, 0, width.div_euclid(2), height))
        .map_err(|_| "Left roi invalid.")?;
    let _ = &hero1_img.copy_to(left_roi.borrow_mut());

    let mut right_roi = hero_frame
        .roi_mut(Rect::new(
            width.div_euclid(2),
            0,
            width.div_euclid(2),
            height,
        ))
        .map_err(|_| "Right roi invalid")?;
    let _ = &hero2_img.copy_to(right_roi.borrow_mut())?;

    let exude = imgcodecs::imread(&EXUDE_FILE, imgcodecs::IMREAD_COLOR).unwrap();
    let mut exude_frame = Mat::default();
    resize(
        &exude,
        &mut exude_frame,
        Size::new(width, height),
        0.,
        0.,
        0,
    )?;

    let mut mat_img = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    hero_frame.copy_to(&mut mat_img)?;
    overlay_video_sectional_with_fade(
        &mut out,
        FIRE_OVERLAY,
        &mat_img,
        10,
        fade_in_count,
        hero_frame_count,
        fade_out_count,
    )?;

    Ok(())
}
