use opencv::{
    boxed_ref::BoxedRefMut,
    core::{
        add_weighted, bitwise_and, bitwise_not, bitwise_not_def, bitwise_or, bitwise_or_def, in_range, no_array, Rect, Scalar, ToInputArray, UMat, UMatTrait, UMatTraitConst
    },
};

const COLOR_LENIENCY: f64 = 80.0;

fn determine_region_fade_percentage(
    roi: &BoxedRefMut<UMat>,
    target_color: &Scalar,
) -> Result<f64, Box<dyn std::error::Error>> {
    let mean = opencv::core::mean_def(roi)?;
    let avg_rgb = ((target_color[0] - (mean[0] as f64)).abs()
        + (target_color[1] - mean[1] as f64).abs()
        + (target_color[2] - (mean[2] as f64)).abs())
        / 3.0;
    // The closer to 0, the darker the pixel
    let fade_percent = avg_rgb / 255.0;
    Ok(fade_percent.powf(1.0 / 3.0))
}

pub fn overlay_image_sectional_with_fade(
    background: &UMat,
    foreground: &UMat,
    target_color: &Scalar,
    pixels: i32,
) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut background = background.clone();
    let mut foreground = foreground.clone();

    let height = foreground.size()?.height;
    let width = foreground.size()?.width;

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
            let origin_video_roi = background.roi(rect)?.try_clone()?;
            let mut video_roi = background.roi_mut(rect)?;

            let foreground_roi = foreground.roi_mut(rect)?;

            let fade_factor = determine_region_fade_percentage(&foreground_roi, target_color)?;
            add_weighted(
                &foreground_roi,
                fade_factor,
                &origin_video_roi,
                1.0 - fade_factor,
                0.,
                &mut video_roi,
                0,
            )?;
        }
    }
    Ok(background)
}

pub fn overlay_image_sectional_with_removal(
    background: &UMat,
    foreground: &UMat,
    target_color: &Scalar,
    pixels: i32,
    threshold: f64,
) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut background = background.clone();
    let mut foreground = foreground.clone();

    let height = foreground.size()?.height;
    let width = foreground.size()?.width;

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
            let background_roi = background.roi(rect)?.try_clone()?;
            let mut video_roi = background.roi_mut(rect)?;

            let foreground_roi = foreground.roi_mut(rect)?;

            let fade_factor = determine_region_fade_percentage(&foreground_roi, target_color)?;
            if fade_factor > threshold {
                foreground_roi.copy_to(&mut video_roi)?;
            } else {
                background_roi.copy_to(&mut video_roi)?;
            }
        }
    }
    Ok(background)
}

pub fn overlay_image_roi_sectional_with_removal(
    background: &UMat,
    smaller: &UMat,
    target_color: &Scalar,
    pixels: i32,
    threshold: f64,
    roi: Rect,
) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut smaller = smaller.clone();
    let mut section = UMat::new_size_def(roi.size(), background.typ()).unwrap();

    let height = roi.height;
    let width = roi.width;

    for y in 0..height.div_euclid(pixels) {
        for x in 0..width.div_euclid(pixels) {
            let total_roi_width = width - pixels * x;
            let total_roi_height = height - pixels * y;

            let roi_width = pixels.min(total_roi_width);
            let roi_height = pixels.min(total_roi_height);

            let background_rect = Rect::new(
                roi.x + pixels * x,
                roi.y + pixels * y,
                roi_width,
                roi_height,
            );
            let background_roi_clone = background.roi(background_rect)?.try_clone()?;
            let smaller_rect = Rect::new(pixels * x, pixels * y, roi_width, roi_height);

            let foreground_roi = smaller.roi_mut(smaller_rect)?;

            let fade_factor = determine_region_fade_percentage(&foreground_roi, target_color)?;
            if fade_factor > threshold {
                foreground_roi.copy_to(&mut section)?;
            } else {
                background_roi_clone.copy_to(&mut section)?;
            }
        }
    }
    Ok(section)
}

pub fn remove_color(
    background: &impl ToInputArray,
    foreground: &UMat,
    target_color: &Scalar,
) -> Result<UMat, Box<dyn std::error::Error>> {
    let lower_bound = Scalar::new(
        target_color[0] - COLOR_LENIENCY,
        target_color[1] - COLOR_LENIENCY,
        target_color[2] - COLOR_LENIENCY,
        target_color[3] - COLOR_LENIENCY
    );
    let upper_bound = Scalar::new(
        target_color[0] + COLOR_LENIENCY,
        target_color[1] + COLOR_LENIENCY,
        target_color[2] + COLOR_LENIENCY,
        target_color[3] + COLOR_LENIENCY,
    );

    let mut out_mask = UMat::new_def();
    let mut in_mask = UMat::new_def();
    // calculate sub-array to remove by finding colors within the spectrum
    in_range(foreground, &lower_bound, &upper_bound, &mut out_mask)?;

    // invert the sub-array to find the parts of the original image should remain
    bitwise_not_def(&out_mask, &mut in_mask).expect("Bitwise not failed");

    let mut out = UMat::new_def();
    let mut inn = UMat::new_def();
    // Create array of background pixels using the mask of pixels to be removed
    bitwise_and(background, background, &mut out, &out_mask)
        .expect("Bitwise-and for out mask failed");
    // Create array of foreground pixels that will be kept
    bitwise_and(foreground, foreground, &mut inn, &in_mask)
        .expect("Bitwise-and for in mask failed");
    // Mash the two previous arrays together
    bitwise_or_def(&out.clone(), &inn, &mut out).expect("Bitwise-or failed");

    Ok(out)
}

pub fn remove_white_corners(
    background: &impl ToInputArray,
    foreground: &UMat,
) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut out_mask = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    let mut in_mask = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);

    in_range(
        foreground,
        &Scalar::new(100.0, 100.0, 100.0, 0.0),
        &Scalar::new(255.0, 255.0, 255.0, 0.0),
        &mut out_mask,
    )?;
    let mut edge_mask = UMat::zeros(out_mask.rows(), out_mask.cols(), out_mask.typ())?;
    let col_increment = out_mask.cols().div_euclid(25);
    let row_increment = out_mask.rows().div_euclid(30);

    for x in 0..col_increment {
        edge_mask.col_mut(x)?.set_to(&255.0, &mut no_array())?;
        edge_mask
            .col_mut(foreground.cols() - (x + 1))?
            .set_to(&255.0, &mut no_array())?;
    }

    for x in 0..row_increment {
        edge_mask.row_mut(x)?.set_to(&255.0, &mut no_array())?;
        edge_mask
            .row_mut(foreground.rows() - (x + 1))?
            .set_to(&255.0, &mut no_array())?;
    }

    bitwise_and(
        &out_mask.clone(),
        &edge_mask,
        &mut out_mask,
        &mut no_array(),
    )
    .expect("bitwise-and in remove_white_corners failed");

    bitwise_not(&out_mask, &mut in_mask, &no_array())
        .expect("bitwise_not in remove_white_corners failed");

    let mut out = UMat::new_def();
    let mut inn = UMat::new_def();

    bitwise_and(foreground, foreground, &mut inn, &in_mask)
        .expect("bitwise-and for in mask in remove_white_corners failed");
    bitwise_and(background, background, &mut out, &out_mask)
        .expect("bitwise-and for out mask in remove_white_corners failed");
    bitwise_or(&out.clone(), &inn, &mut out, &no_array())
        .expect("bitwise_or failed for in/ or mask in remove_white_corners");

    Ok(out)
}

pub fn convert_alpha_to_white(image: &UMat) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut alpha_mask = UMat::new_def();

    in_range(
        image,
        &Scalar::new(0.0, 0.0, 0.0, 0.0),
        &Scalar::new(255.0, 255.0, 255.0, 0.0),
        &mut alpha_mask,
    )?;

    let mut new = UMat::new_def();
    let white_mat = UMat::new_rows_cols_with_default_def(
        image.rows(),
        image.cols(),
        image.typ(),
        Scalar::new(255.0, 255.0, 255.0, 255.0),
    )?;
    bitwise_or(&image, &white_mat, &mut new, &alpha_mask)
        .expect("converting alpha to white mask failed");
    bitwise_or(&image, &new.clone(), &mut new, &no_array()).expect("copying image to white failed");

    Ok(new)
}


#[cfg(test)]
mod test {
    use opencv::{core::{Scalar, Size, UMat, UMatTraitConst, Vector, CV_8U}, highgui::{wait_key, wait_key_ex_def}, imgcodecs::{imwrite, ImwriteFlags}, imgproc::{cvt_color_def, COLOR_BGR2BGRA, COLOR_BGRA2BGR, COLOR_RGBA2RGB}, viz::imshow_def};

    use crate::{image::load_image, movement::resize_umat, rotate::rotate_image};

    use super::{convert_alpha_to_white, remove_color, remove_white_corners};

    #[test]
    fn test_remove_color() -> Result<(), Box<dyn std::error::Error>> {
        let fp = "data/remove.png";
        // std::fs::remove_file(&fp)?;

        let size = Size::new(500, 700);
        let mut card_back = load_image("../data/cardback.png")?;
        cvt_color_def(&card_back.clone(), &mut card_back, COLOR_BGRA2BGR)?;
        let alpha = UMat::new_size_with_default_def(card_back.size()?, card_back.typ(), Scalar::new(252.0, 116.0, 5.0, 0.0))?;
        let card_back = remove_white_corners(&alpha, &card_back)?;

        let img = resize_umat(&card_back, &size)?;
        let img = rotate_image(&img, 0.5, true)?;
        let background = UMat::new_size_with_default_def(img.size()?, card_back.typ(), Scalar::new(0.0, 0.0, 255.0, 0.0))?;

        let out = remove_color(&background, &img, &Scalar::new(252.0, 116.0, 5.0, 0.0))?;
        let mut params = Vector::new();
        params.push(ImwriteFlags::IMWRITE_PNG_COMPRESSION as i32);
        params.push(9);

        imwrite(fp, &out, &params)?;
        // imwrite(fp, &img, &params)?;

        Ok(())
    }
}
