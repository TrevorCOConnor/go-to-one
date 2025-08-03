use std::{borrow::BorrowMut, collections::HashMap};

use opencv::{
    core::{MatTraitConst, Rect, Size, UMat, UMatTrait, UMatTraitConst},
    imgcodecs, imgproc, Error,
};

use crate::err::RoiError;

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

pub fn load_image(fp: &str) -> Result<UMat, opencv::error::Error> {
    let mut umat = UMat::new_def();
    let img = imgcodecs::imread(fp, imgcodecs::IMREAD_COLOR)?;
    img.copy_to(&mut umat)?;

    Ok(umat)
}

pub fn load_image_unchanged(fp: &str) -> Result<UMat, opencv::error::Error> {
    let mut umat = UMat::new_def();
    // let img = imgcodecs::imread(fp, imgcodecs::IMREAD_COLOR)?;
    let img = imgcodecs::imread(fp, imgcodecs::IMREAD_UNCHANGED)?;
    img.copy_to(&mut umat)?;

    Ok(umat)
}

pub struct FullArtHeroManager {
    map: HashMap<String, String>,
}

impl FullArtHeroManager {
    pub fn new() -> Self {
        Self {
            map: load_full_art_hero_map(),
        }
    }

    /// Loads the hero art animation for a given hero
    pub fn get_hero_art_animation_fp(
        &self,
        hero_name: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(fp) = self.map.get(hero_name) {
            Ok(format!("data/full_art_heroes/{}", fp))
        } else {
            Err(Box::new(Error::new(500, format!("Could not find full art animation for hero '{}' in the config file. An update is likely needed.", hero_name))))
        }
    }

    /// Loads the hero art animation for a given hero
    pub fn get_cropped_hero_art_animation_fp(
        &self,
        hero_name: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(fp) = self.map.get(hero_name) {
            Ok(format!("data/full_art_heroes/cropped_{}", fp))
        } else {
            Err(Box::new(Error::new(500, format!("Could not find full art animation for hero '{}' in the config file. An update is likely needed.", hero_name))))
        }
    }

    /// Loads only the top half fo the hero art animation
    pub fn crop_hero_img(hero_mat: &UMat) -> Result<UMat, Box<dyn std::error::Error>> {
        let roi = hero_mat.roi(Rect::new(
            0,
            0,
            hero_mat.size()?.width,
            ((hero_mat.size()?.height) as f64 * (2.0 / 3.0)) as i32,
        ))?;

        let mut half_frame = UMat::new_def();
        roi.copy_to(&mut half_frame)?;
        Ok(half_frame)
    }
}

fn load_full_art_hero_map() -> HashMap<String, String> {
    let file = std::fs::File::open("data/full_art_hero_map.json")
        .expect("Can't find full art hero json file.");
    let json: HashMap<String, String> =
        serde_json::from_reader(file).expect("Full art json file incorrectly formatted.");
    json
}

pub fn copy_to(
    img: &UMat,
    background: &mut UMat,
    roi: &Rect,
) -> Result<(), Box<dyn std::error::Error>> {
    if img.size()?.width != roi.width {
        return Err(RoiError::TooWide.into());
    }

    if img.size()?.height != roi.height {
        return Err(RoiError::TooTall.into());
    }

    let mut roi_ref = background.roi_mut(*roi)?;
    img.copy_to(roi_ref.borrow_mut())?;
    Ok(())
}

pub fn crop(img: &UMat, roi: &Rect) -> Result<UMat, Box<dyn std::error::Error>> {
    let mut cropped = UMat::new_def();
    let crop_roi = img.roi(*roi)?;
    crop_roi.copy_to(&mut cropped)?;
    Ok(cropped)
}
