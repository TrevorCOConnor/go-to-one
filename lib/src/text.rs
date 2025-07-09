use std::borrow::BorrowMut;

use opencv::{
    core::{Point, Scalar, Size, UMat, UMatTrait, UMatTraitConst, VecN},
    imgproc::{get_text_size, put_text, LINE_8},
};

use crate::{fade::remove_color, relative_roi::RelativeRoi};

/// Centers text within the UMat at given rect
pub fn center_text_at(
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
