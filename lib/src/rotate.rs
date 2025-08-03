use opencv::{
    calib3d::{ find_homography_def},
    core::{no_array, Point2f, Scalar, Size, UMat, Vector, BORDER_CONSTANT},
    imgproc::{cvt_color_def, warp_perspective, COLOR_RGBA2RGB, INTER_NEAREST},
    prelude::*,
};
use std::error::Error;
use std::f32::consts::E;

const CARD_HEIGHT_EXT: f32 = 0.08;
// Bright blue
pub const REMOVAL_COLOR: Scalar = Scalar::new(252.0, 116.0, 5.0, 0.0);

fn rotate_function(percent: f32) -> f32 {
    let scalar = (E.powi(2) - 1.0).recip();
    scalar * (E.powf(2.0 * percent) - 1.0)
}

pub fn rotate_image(
    image: &UMat,
    percentage: f32,
    rotate_out: bool,
) -> Result<UMat, Box<dyn Error>> {
    let width = image.cols() as f32;
    let height = image.rows() as f32;

    let percentage = {
        if rotate_out {
            percentage + 0.01
        } else {
            0.99 - percentage // need to look into this, but it does not like that value 1.0
        }
    };

    let percentage = rotate_function(percentage);

    // As card rotates, the width will go to 0
    let new_width = width * (1.0 - percentage);

    // Calculate the diff in the new width and old to keep the rotating card centered
    let width_diff = (width - new_width) * 0.5;

    // The height of the rotating card will change by a percentage on each side
    let height_offset = height * (CARD_HEIGHT_EXT * percentage);

    let base_height = CARD_HEIGHT_EXT * height * 0.5;

    // xs
    let left_x = width_diff;
    let right_x = width_diff + new_width;

    // ys
    let top_extended_y = base_height - height_offset;
    let top_reduced_y = base_height + height_offset;
    let bottom_extended_y = base_height + height + height_offset;
    let bottom_reduced_y = base_height + height - height_offset;

    // origin
    let src_points = Vector::<Point2f>::from_slice(&[
        Point2f::new(0.0, 0.0),      // Top-left
        Point2f::new(width, 0.0),    // Top-right
        Point2f::new(width, height), // Bottom-right
        Point2f::new(0.0, height),   // Bottom-left
    ]);

    // destination
    let dst_points = {
        // Rotating in goes makes the left side bigger and the right side smaller
        if rotate_out {
            Vector::<Point2f>::from_slice(&[
                Point2f::new(left_x, top_extended_y),    // Top-left
                Point2f::new(right_x, top_reduced_y),    // Top-right
                Point2f::new(right_x, bottom_reduced_y), // Bottom-right
                Point2f::new(left_x, bottom_extended_y), // Bottom-left
            ])
        // Rotating in goes makes the left side bigger and the right side smaller
        } else {
            Vector::<Point2f>::from_slice(&[
                Point2f::new(left_x, top_reduced_y),      // Top-left
                Point2f::new(right_x, top_extended_y),    // Top-right
                Point2f::new(right_x, bottom_extended_y), // Bottom-right
                Point2f::new(left_x, bottom_reduced_y),   // Bottom-left
            ])
        }
    };

    // output
    let output_size = Size::new(width as i32, ((1.0 + CARD_HEIGHT_EXT) * height) as i32);

    // Calculate the homography
    let homography = find_homography_def(&src_points, &dst_points, &mut no_array())?;

    // Warp the image for the current frame
    let mut warped_frame = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    warp_perspective(
        &image,
        &mut warped_frame,
        &homography,
        output_size,
        INTER_NEAREST,
        BORDER_CONSTANT,
        REMOVAL_COLOR
    )?;

    cvt_color_def(&warped_frame.clone(), &mut warped_frame, COLOR_RGBA2RGB)?;

    Ok(warped_frame)
}
