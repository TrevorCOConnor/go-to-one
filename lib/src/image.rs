use opencv::{
    core::{Rect, Size, UMat, UMatTraitConst},
    imgproc, Error,
};

const ART_RATIO: f64 = 3.0 / 5.0;
const BORDER_X_RATIO: f64 = 1.0 / 30.0;
const BORDER_Y_RATIO: f64 = 1.0 / 36.0;

/// Gets just the card art from the image of a card.
/// Currently this relies on hard coded ratios and will not work with EA cards or meld cards.
pub fn get_card_art(image: &UMat, card_width: i32, card_height: i32) -> Result<UMat, Error> {
    // Resize card to match frame ratio
    let mut resized = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    imgproc::resize(
        &image,
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
    let mut cropped = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    // let _ = Mat::roi(&img, roi)?.copy_to(&mut cropped)?;
    UMat::roi(&resized, roi)?.copy_to(&mut cropped)?;

    Ok(cropped)
}

/// LINEAR
fn linear_progression(b: f64, percentage: f64) -> f64 {
    (1.0 - b) * percentage + b
}

/// All functions that can be used to calculate the progression of the image from card art to full
/// card
/// LINEAR: Constant speed
pub enum ProgressionFunction {
    LINEAR,
}

impl ProgressionFunction {
    fn apply(&self, b: f64, percentage: f64) -> f64 {
        match &self {
            ProgressionFunction::LINEAR => linear_progression(b, percentage),
        }
    }
}

/// At 0.0 returns just the card art. At 1.0 returns the whole card. `progression_func` is a
/// function that determines the images generated between 0.0 and 1.0
pub fn get_card_art_progressive(
    image: &UMat,
    percentage: f64,
    progression_func: ProgressionFunction,
) -> Result<UMat, Error> {
    // Create scalars based on percentage
    let art_scalar = progression_func.apply(ART_RATIO, percentage);
    let border_x_scalar = progression_func.apply(BORDER_X_RATIO, percentage);
    let border_y_scalar = progression_func.apply(BORDER_Y_RATIO, percentage);

    // Create a Rect object to represent the ROI
    let art_height = ((image.rows() as f64) * art_scalar) as i32;
    let border_x_offset = ((image.cols() as f64) * border_x_scalar) as i32;
    let border_y_offset = ((image.rows() as f64) * border_y_scalar) as i32;
    let roi = Rect::new(
        border_x_offset,
        border_y_offset,
        image.cols() - (2 * border_x_offset),
        art_height,
    );

    // Crop the image using the ROI
    let mut cropped = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
    UMat::roi(image, roi)?.copy_to(&mut cropped)?;

    Ok(cropped)
}
