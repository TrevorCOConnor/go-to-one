use opencv::{
    core::{Mat, MatTraitConst, Rect, Size},
    imgcodecs, imgproc, Error,
};

const ART_RATIO: f64 = 3.0 / 5.0;
const BORDER_X_RATIO: f64 = 1.0 / 30.0;
const BORDER_Y_RATIO: f64 = 1.0 / 36.0;

pub fn get_card_art(image_fp: &str, card_width: i32, card_height: i32) -> Result<Mat, Error> {
    // Load the image
    let img = imgcodecs::imread(image_fp, imgcodecs::IMREAD_COLOR)?;

    // Check if the image was loaded successfully
    if img.empty() {
        panic!("Could not open or find the image!");
    }
    // Resize card to match frame ratio
    let mut resized = Mat::default();
    imgproc::resize(
        &img,
        &mut resized,
        Size::new(card_width, card_height),
        0.0,
        0.0,
        imgproc::INTER_LINEAR,
    )?;

    // Create a Rect object to represent the ROI
    let art_height = ((resized.rows() as f64) * ART_RATIO) as i32;
    let border_x_offset = ((resized.cols() as f64) * BORDER_X_RATIO) as i32;
    let border_y_offset = ((resized.rows() as f64) * BORDER_Y_RATIO) as i32;
    let roi = Rect::new(
        border_x_offset,
        border_y_offset,
        resized.cols() - (2 * border_x_offset),
        art_height,
    );

    // Crop the image using the ROI
    let mut cropped = Mat::default();
    // let _ = Mat::roi(&img, roi)?.copy_to(&mut cropped)?;
    Mat::roi(&resized, roi)?.copy_to(&mut cropped)?;

    Ok(cropped)
}
