use std::borrow::BorrowMut;

use opencv::{
    core::{Point, Rect, Scalar, Size, UMat, UMatTrait, UMatTraitConst, VecN},
    imgproc::{get_text_size, put_text, resize_def, LINE_8},
};

use crate::{
    fade::remove_color,
    relative_roi::{center_offset, RelativeRoi},
};

/// Centers text within the UMat at given rect
pub fn center_text_at_rel(
    frame: &mut UMat,
    text: &str,
    font_face: i32,
    font_scale: f64,
    color: VecN<f64, 4>,
    thickness: i32,
    rel_roi: RelativeRoi,
    buffer: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut baseline = 0;
    let text_size = get_text_size(text, font_face, font_scale, thickness, &mut baseline)?;

    let mut text_umat = UMat::new_size_with_default_def(
        Size::new(text_size.width + buffer, text_size.height + buffer),
        frame.typ(),
        Scalar::new(0.0, 0.0, 0.0, 0.0),
    )?;
    put_text(
        &mut text_umat,
        &text,
        Point::new(
            buffer.div_euclid(2),
            text_size.height + buffer.div_euclid(2),
        ),
        font_face,
        font_scale,
        color,
        thickness,
        LINE_8,
        false,
    )?;

    let roi = rel_roi.generate_roi(&frame.size()?, &text_umat);
    let text_umat = rel_roi.resize(&frame.size()?, &text_umat)?;

    let mut roi = frame.roi_mut(roi)?;
    let mut roi_clone = UMat::new_def();
    roi.copy_to(&mut roi_clone)?;

    let new = remove_color(&roi_clone, &text_umat, &Scalar::new(0.0, 0.0, 0.0, 0.0))?;
    new.copy_to(roi.borrow_mut())?;

    Ok(())
}

pub fn center_text_at_rect(
    frame: &mut UMat,
    text: &str,
    font_face: i32,
    font_scale: f64,
    color: VecN<f64, 4>,
    thickness: i32,
    rect: Rect,
    buffer: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut baseline = 0;
    let text_size = get_text_size(text, font_face, font_scale, thickness, &mut baseline)?;

    let mut text_umat = UMat::new_size_with_default_def(
        Size::new(text_size.width + buffer, text_size.height + buffer),
        frame.typ(),
        Scalar::new(0.0, 0.0, 0.0, 0.0),
    )?;
    put_text(
        &mut text_umat,
        &text,
        Point::new(
            buffer.div_euclid(2),
            text_size.height + buffer.div_euclid(2),
        ),
        font_face,
        font_scale,
        color,
        thickness,
        LINE_8,
        false,
    )?;

    let ratio = text_umat.size()?.width as f64 / text_umat.size()?.height as f64;

    // calculate potential dimensions based on ratio
    let potential_height = rect.width as f64 * ratio.recip();
    let potential_width = rect.height as f64 * ratio;

    // determine actual dimensions based on all restrictions
    let (width, height) = {
        if potential_width > rect.width as f64 {
            (rect.width, potential_height as i32)
        } else {
            (potential_width as i32, rect.height)
        }
    };

    let mut resized = UMat::new_def();
    resize_def(&text_umat, &mut resized, Size::new(width, height))?;

    let roi = Rect::new(
        rect.x + center_offset(width, rect.width),
        rect.y + center_offset(height, rect.height),
        width,
        height,
    );

    let mut roi = frame.roi_mut(roi)?;

    let new = remove_color(&roi, &resized, &Scalar::new(0.0, 0.0, 0.0, 0.0))?;
    new.copy_to(roi.borrow_mut())?;

    Ok(())
}
