use std::{
    borrow::BorrowMut,
    f64::consts::{E, PI},
    ops::{Add, Mul, Sub},
};

use crate::{err::RoiError, relative_roi::center_offset};
use opencv::{
    core::{Point, Rect, Size, UMat, UMatTrait, UMatTraitConst},
    imgproc::resize_def,
};

/// 1/(x+1) ish
fn rush_to_one(percentage: f64) -> f64 {
    let multiplier = 10.0;
    1.0 - 1.0 / (multiplier * percentage + 1.0)
}

fn s_curve(percentage: f64) -> f64 {
    let t = straight_line(-6.0, 6.0, percentage);
    (1.0 + E.powf(-t)).recip()
}

fn arctan_ish(percentage: f64) -> f64 {
    let left = -5.0;
    let right = 10.0;
    (f64::atan(left + (right - left) * percentage) - f64::atan(left))
        / (f64::atan(right) - f64::atan(left))
}

/// piecewise (1/k)sin(x)
fn bounce(percentage: f64) -> f64 {
    let bounces = 4.0;
    let t = straight_line(0.0, bounces, percentage);
    let k = t.floor();

    if k == 0.0 {
        straight_line(0.0, 1.0, t)
    } else {
        let frac = t.fract();
        let r = straight_line(PI, 2.0 * PI, frac);
        1.0 + (1.0 / ((k + 1.0).powf(4.0))) * f64::sin(r)
    }
}

pub enum Reparameterization {
    RushToOne,
    ArcTan,
    SCurve,
    Bounce,
}

impl Reparameterization {
    pub fn apply(&self, percentage: f64) -> f64 {
        match self {
            Reparameterization::RushToOne => rush_to_one(percentage),
            Reparameterization::ArcTan => arctan_ish(percentage),
            Reparameterization::SCurve => s_curve(percentage),
            Reparameterization::Bounce => bounce(percentage),
        }
    }
}

pub fn straight_line<T: Sub<Output = T> + Add<Output = T> + Mul<f64, Output = T> + Copy>(
    start: T,
    end: T,
    percentage: f64,
) -> T {
    (end - start) * percentage + start
}

/// linear
fn linear_move(start: &Point, end: &Point, percentage: f64) -> Point {
    let diff = *end - *start;
    Point::new(
        (diff.x as f64 * percentage) as i32,
        (diff.y as f64 * percentage) as i32,
    )
}

/// arctan
pub fn slow_fast_slow_curve(start: &Point, end: &Point, percentage: f64) -> Point {
    let t = Reparameterization::ArcTan.apply(percentage);
    let x = (end.x as f64 - start.x as f64) * t + start.x as f64;
    let y = (end.y as f64 - start.y as f64) * t.powf(3.0) + start.y as f64;
    Point::new(x as i32, y as i32)
}

/// All functions that can be used to move an image
/// LINEAR: Straight line with constant speed
pub enum MoveFunction {
    Linear,
    SlowFastSlowCurve,
}

impl MoveFunction {
    fn apply(&self, start: &Point, end: &Point, percentage: f64) -> Point {
        match self {
            MoveFunction::Linear => linear_move(start, end, percentage),
            MoveFunction::SlowFastSlowCurve => slow_fast_slow_curve(start, end, percentage),
        }
    }
}

/// Moves the hero from `start_location` to `end_location` using the specified functions.
/// Note: This function does not _remove_ the image from the frame. The frame should not have the
/// image in it when this is called.
pub fn move_umat(
    start_location: &Point,
    end_location: &Point,
    img: &UMat,
    frame: &mut UMat,
    percentage: f64,
    move_func: MoveFunction,
) -> Result<(), Box<dyn std::error::Error>> {
    let rect = relocate_umat(
        start_location,
        end_location,
        img,
        frame,
        percentage,
        move_func,
    )?;
    let mut roi = frame.roi_mut(rect)?;
    img.copy_to(roi.borrow_mut())?;

    Ok(())
}

pub fn relocate_umat(
    start_location: &Point,
    end_location: &Point,
    img: &UMat,
    frame: &mut UMat,
    percentage: f64,
    move_func: MoveFunction,
) -> Result<Rect, Box<dyn std::error::Error>> {
    // verify percentage is valid
    if percentage < 0.0 || 1.0 < percentage {
        panic!("percentage is invalid")
    }

    // calculate new location
    let location = move_func.apply(&start_location, &end_location, percentage);

    if location.y < 0 || location.x < 0 {
        panic!("negative location value")
    }

    // Check that ROI is valid
    if frame.size()?.width < location.x as i32 + img.size()?.width {
        return Err(Box::new(RoiError::TooWide));
    }

    // Check that ROI is valid
    if frame.size()?.height < location.y as i32 + img.size()?.height {
        panic!(
            "location: {}; height: {}",
            location.y,
            img.size().unwrap().height
        )
        // return Err(Box::new(RoiError::TooTall));
    }

    let roi = Rect::new(
        location.x,
        location.y,
        img.size()?.width,
        img.size()?.height,
    );

    Ok(roi)
}

pub fn safe_scale(
    current: &Rect,
    frame_size: &Size,
    scale: f64,
) -> Result<Rect, Box<dyn std::error::Error>> {
    if scale <= 0.0 {
        return Err(RoiError::NegativeScale.into());
    }

    if scale <= 1.0 {
        return Ok(scale_rect(current, scale));
    }

    let x = current.x as f64;
    let y = current.y as f64;
    let width = current.width as f64;
    let height = current.height as f64;
    let frame_width = frame_size.width as f64;
    let frame_height = frame_size.height as f64;

    let left_bound = (width + 2.0 * x) / width;
    let right_bound = (width + 2.0 * (frame_width - x - width)) / width;
    let top_bound = (height + 2.0 * y) / height;
    let bottom_bound = (height + 2.0 * (frame_height - y - height)) / height;

    let minimum = [left_bound, right_bound, top_bound, bottom_bound, scale]
        .into_iter()
        .reduce(f64::min)
        .unwrap();

    Ok(scale_rect(current, minimum))
}

pub fn scale_rect(current: &Rect, scale: f64) -> Rect {
    let new_width = (current.width as f64 * scale) as i32;
    let new_height = (current.height as f64 * scale) as i32;
    let x_offset = center_offset(current.width, new_width);
    let y_offset = center_offset(current.height, new_height);

    let new_x = current.x - x_offset;
    let new_y = current.y - y_offset;

    Rect::new(new_x, new_y, new_width, new_height)
}

pub fn resize_umat(umat: &UMat, new_size: &Size) -> Result<UMat, opencv::Error> {
    let mut resized = UMat::new_def();
    resize_def(&umat, &mut resized, *new_size)?;
    Ok(resized)
}

pub fn place_umat(umat: &UMat, frame: &mut UMat, rect: Rect) -> Result<(), opencv::Error> {
    let mut roi = frame.roi_mut(rect)?;
    umat.copy_to(roi.borrow_mut())
}

#[cfg(test)]
mod test {

    use super::*;
    use opencv::{
        core::{Point, Scalar, Size, UMat, CV_8UC3},
        videoio::{VideoWriter, VideoWriterTrait},
    };

    use crate::image::load_image;

    #[test]
    fn test_linear() -> Result<(), Box<dyn std::error::Error>> {
        let fps = 30;
        let time = 2;
        let frames = time * fps;
        let mut writer = VideoWriter::new_def(
            "data/test/linear_test.mp4",
            VideoWriter::fourcc('m', 'p', '4', 'v')?,
            30.0,
            Size::new(1920, 1080),
        )?;

        let card_img_fp = std::env::current_dir()?
            .parent()
            .unwrap()
            .join("data/cardback.png");
        let card_image = load_image(card_img_fp.to_str().unwrap())?;

        for i in 0..frames {
            let mut frame = UMat::new_size_with_default_def(
                Size::new(1920, 1080),
                CV_8UC3,
                Scalar::new(0.0, 0.0, 0.0, 0.0),
            )?;
            move_umat(
                &Point::new(0, 0),
                &Point::new(1000, 200),
                &card_image,
                &mut frame,
                i as f64 / frames as f64,
                MoveFunction::Linear,
            )?;
            writer.write(&frame)?;
        }

        Ok(())
    }

    #[test]
    fn test_curve() -> Result<(), Box<dyn std::error::Error>> {
        let fps = 60;
        let time = 2;
        let frames = time * fps;
        let mut writer = VideoWriter::new_def(
            "data/test/curve_test.mp4",
            VideoWriter::fourcc('m', 'p', '4', 'v')?,
            30.0,
            Size::new(1920, 1080),
        )?;

        let card_img_fp = std::env::current_dir()?
            .parent()
            .unwrap()
            .join("data/cardback.png");
        let card_image = load_image(card_img_fp.to_str().unwrap())?;

        for i in 0..frames {
            let mut frame = UMat::new_size_with_default_def(
                Size::new(1920, 1080),
                CV_8UC3,
                Scalar::new(0.0, 0.0, 0.0, 0.0),
            )?;
            move_umat(
                &Point::new(0, 0),
                &Point::new(1000, 200),
                &card_image,
                &mut frame,
                i as f64 / frames as f64,
                MoveFunction::SlowFastSlowCurve,
            )?;
            writer.write(&frame)?;
        }

        Ok(())
    }
}
