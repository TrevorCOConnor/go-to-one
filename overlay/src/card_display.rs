use std::collections::VecDeque;

use lib::{card::CardImageDB, fade::{remove_color, remove_white_corners}, movement::{place_umat, relocate_umat, resize_umat, safe_scale, straight_line, MoveFunction, Reparameterization}, relative_roi::center_offset, rotate::rotate_image};
use opencv::core::{Rect, Scalar, UMat, UMatTrait, UMatTraitConst, Point};

use crate::{DataRow, TimeTick, DISPLAY_DURATION, EXTENDED_DISPLAY_DURATION, FADE_OUT_DURATION, GREEN, POST_ZOOM_TIME, ROTATE_TIME, ZOOM, ZOOM_DISPLAY, ZOOM_TIME};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

enum CardDisplayPhase {
    CardBackRotateOut,
    CardFrontRotateIn,
    Display,
    Extended,
    CardFrontRotateOut,
    CardBackRotateIn,
    Sleep,
    ZoomIn,
    ZoomDisplay,
    ZoomOut,
    PostZoom,
}

pub struct CardDisplayManager {
    card_rect: Rect,
    card_db: lib::card::CardImageDB,
    card_back: UMat,
    display_card: Option<UMat>,
    phase: CardDisplayPhase,
    queue: VecDeque<DataRow>,
    timer: TimeTick,
    zoom: bool,
}

impl CardDisplayManager {
    pub fn queue_zoom(&mut self) {
        if self.display_card.is_some() {
            self.queue.push_back(DataRow {
                update_type: ZOOM.to_owned(),
                ..Default::default()
            });
        }
    }

    pub fn add_card_to_queue(&mut self, card: DataRow) {
        self.queue.push_back(card);
    }

    pub fn new(card_rect: &Rect, card_back: &UMat, time_tick: &TimeTick) -> Self {
        let card_db = CardImageDB::init();
        Self {
            card_rect: card_rect.clone(),
            card_db,
            card_back: card_back.clone(),
            display_card: None,
            phase: CardDisplayPhase::Sleep,
            queue: VecDeque::new(),
            timer: time_tick.clone(),
            zoom: false,
        }
    }

    pub fn tick(&mut self, time_tick: TimeTick, frame: &mut UMat, frame_rect: &Rect) -> Result<()> {
        let elapsed_time = (time_tick - self.timer).as_f64();

        // Check for zoom
        if self.queue.len() > 0 {
            if self.queue.front().as_ref().unwrap().update_type == ZOOM {
                self.queue.pop_front();
                // ignore zooms not attached to a card
                if self.display_card.is_some() {
                    self.zoom = true;
                }
            }
        }
        match self.phase {
            CardDisplayPhase::CardBackRotateOut => {
                if elapsed_time >= ROTATE_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::CardFrontRotateIn;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let t = elapsed_time / ROTATE_TIME;
                    let rotated = rotate_image(&self.card_back, t as f32, true)?;
                    let rotated_rect = Rect::new(
                        self.card_rect.x,
                        self.card_rect.y
                            - center_offset(self.card_rect.height, rotated.size()?.height),
                        rotated.size()?.width,
                        rotated.size()?.height,
                    );

                    let roi = &frame.roi(rotated_rect)?;

                    let card_rotation =
                        remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                    let mut inner_roi = frame.roi_mut(rotated_rect)?;
                    card_rotation.copy_to(&mut inner_roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::CardFrontRotateIn => {
                if elapsed_time >= ROTATE_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::Display;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let t = elapsed_time / ROTATE_TIME;
                    let display_card = self.display_card.as_ref().unwrap();
                    let green = UMat::new_size_with_default_def(
                        display_card.size()?,
                        display_card.typ(),
                        GREEN,
                    )?;
                    let card = remove_white_corners(&green, &display_card)?;

                    let rotated = rotate_image(&card, t as f32, false)?;
                    let rotated_rect = Rect::new(
                        self.card_rect.x,
                        self.card_rect.y - (rotated.rows() - self.card_rect.height).div_euclid(2),
                        rotated.cols(),
                        rotated.rows(),
                    );

                    let mut roi = frame.roi_mut(rotated_rect)?;
                    let card_rotation =
                        remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                    card_rotation.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::Display => {
                if self.zoom {
                    self.timer = time_tick.clone();
                    self.zoom = false;
                    self.phase = CardDisplayPhase::ZoomIn;
                    self.tick(time_tick, frame, frame_rect)
                } else if elapsed_time >= DISPLAY_DURATION {
                    if self.queue.len() == 0 {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::Extended;
                        self.tick(time_tick, frame, frame_rect)
                    } else {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::CardFrontRotateOut;
                        self.tick(time_tick, frame, frame_rect)
                    }
                } else {
                    let display_card = self.display_card.as_ref().unwrap();
                    let mut roi = frame.roi_mut(self.card_rect)?;

                    let card = remove_white_corners(&roi, &display_card)?;
                    card.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::CardFrontRotateOut => {
                if elapsed_time >= ROTATE_TIME {
                    if self.queue.len() == 0 {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::CardBackRotateIn;
                        self.tick(time_tick, frame, frame_rect)
                    } else {
                        self.timer = time_tick.clone();
                        self.phase = CardDisplayPhase::CardFrontRotateIn;
                        let card = self.queue.pop_front().unwrap();
                        self.load_card_image(&card)?;
                        self.tick(time_tick, frame, frame_rect)
                    }
                } else {
                    let t = elapsed_time / FADE_OUT_DURATION;
                    let display_card = self.display_card.as_ref().unwrap();
                    let green = UMat::new_size_with_default_def(
                        display_card.size()?,
                        display_card.typ(),
                        Scalar::new(0.0, 255.0, 0.0, 0.0),
                    )?;
                    let card = remove_white_corners(&green, &display_card)?;
                    let rotated = rotate_image(&card, t as f32, true)?;
                    let rotated_rect = Rect::new(
                        self.card_rect.x,
                        self.card_rect.y - (rotated.rows() - self.card_rect.height).div_euclid(2),
                        rotated.cols(),
                        rotated.rows(),
                    );

                    let mut roi = frame.roi_mut(rotated_rect)?;

                    let card_rotation =
                        remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                    let card_rotation = remove_white_corners(&roi, &card_rotation)?;

                    card_rotation.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::CardBackRotateIn => {
                if elapsed_time >= ROTATE_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::Sleep;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let t = elapsed_time / ROTATE_TIME;
                    let green = UMat::new_size_with_default_def(
                        self.card_back.size()?,
                        self.card_back.typ(),
                        Scalar::new(0.0, 255.0, 0.0, 0.0),
                    )?;
                    let card = remove_white_corners(&green, &self.card_back)?;

                    let rotated = rotate_image(&card, t as f32, false)?;
                    let rotated_rect = Rect::new(
                        self.card_rect.x,
                        self.card_rect.y - (rotated.rows() - self.card_rect.height).div_euclid(2),
                        rotated.cols(),
                        rotated.rows(),
                    );

                    let mut roi = frame.roi_mut(rotated_rect)?;
                    let card_rotation =
                        remove_color(&roi, &rotated, &Scalar::new(0.0, 255.0, 0.0, 0.0))?;
                    card_rotation.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::ZoomIn => {
                if elapsed_time >= ZOOM_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::ZoomDisplay;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let card = self.display_card.as_ref().unwrap();
                    let percentage = elapsed_time / ZOOM_TIME;
                    let scale_percentage = Reparameterization::SCurve.apply(percentage);

                    let goal_location = Point::new(
                        frame_rect.x + center_offset(self.card_rect.width, frame_rect.width),
                        frame_rect.y + center_offset(self.card_rect.height, frame_rect.height),
                    );

                    let relocation = relocate_umat(
                        &Point::new(self.card_rect.x, self.card_rect.y),
                        &goal_location,
                        &card,
                        frame,
                        percentage,
                        MoveFunction::SlowFastSlowCurve,
                    )?;
                    let resized = safe_scale(
                        &relocation,
                        &frame.size()?,
                        straight_line(1.0, 1.5, scale_percentage),
                    )?;
                    let sized_img = resize_umat(card, &resized.size())?;
                    let roi = frame.roi(resized)?;
                    let sized_img = remove_white_corners(&roi, &sized_img)?;
                    place_umat(&sized_img, frame, resized)?;
                    Ok(())
                }
            }
            CardDisplayPhase::ZoomDisplay => {
                if elapsed_time >= ZOOM_DISPLAY {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::ZoomOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let card = self.display_card.as_ref().unwrap();
                    let scale_percentage = Reparameterization::SCurve.apply(1.0);

                    let goal_location = Point::new(
                        frame_rect.x + center_offset(self.card_rect.width, frame_rect.width),
                        frame_rect.y + center_offset(self.card_rect.height, frame_rect.height),
                    );

                    let relocation = relocate_umat(
                        &Point::new(self.card_rect.x, self.card_rect.y),
                        &goal_location,
                        &card,
                        frame,
                        1.0,
                        MoveFunction::SlowFastSlowCurve,
                    )?;
                    let resized = safe_scale(
                        &relocation,
                        &frame.size()?,
                        straight_line(1.0, 1.5, scale_percentage),
                    )?;
                    let sized_img = resize_umat(card, &resized.size())?;
                    let roi = frame.roi(resized)?;
                    let sized_img = remove_white_corners(&roi, &sized_img)?;
                    place_umat(&sized_img, frame, resized)?;
                    Ok(())
                }
            }
            CardDisplayPhase::ZoomOut => {
                if elapsed_time >= ZOOM_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::PostZoom;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let card = self.display_card.as_ref().unwrap();
                    let percentage = 1.0 - (elapsed_time / ZOOM_TIME);
                    let scale_percentage = Reparameterization::SCurve.apply(percentage);

                    let goal_location = Point::new(
                        frame_rect.x + center_offset(self.card_rect.width, frame_rect.width),
                        frame_rect.y + center_offset(self.card_rect.height, frame_rect.height),
                    );

                    let relocation = relocate_umat(
                        &Point::new(self.card_rect.x, self.card_rect.y),
                        &goal_location,
                        &card,
                        frame,
                        percentage,
                        MoveFunction::SlowFastSlowCurve,
                    )?;
                    let resized = safe_scale(
                        &relocation,
                        &frame.size()?,
                        straight_line(1.0, 1.5, scale_percentage),
                    )?;
                    let sized_img = resize_umat(card, &resized.size())?;
                    let roi = frame.roi(resized)?;
                    let sized_img = remove_white_corners(&roi, &sized_img)?;
                    place_umat(&sized_img, frame, resized)?;
                    Ok(())
                }
            }
            CardDisplayPhase::PostZoom => {
                if elapsed_time >= POST_ZOOM_TIME {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::CardFrontRotateOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let display_card = self.display_card.as_ref().unwrap();
                    let mut roi = frame.roi_mut(self.card_rect)?;

                    let card = remove_white_corners(&roi, &display_card)?;
                    card.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::Extended => {
                if elapsed_time >= EXTENDED_DISPLAY_DURATION || self.queue.len() > 0 {
                    self.timer = time_tick.clone();
                    self.phase = CardDisplayPhase::CardFrontRotateOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let display_card = self.display_card.as_ref().unwrap();
                    let mut roi = frame.roi_mut(self.card_rect)?;

                    let card = remove_white_corners(&roi, &display_card)?;
                    card.copy_to(&mut roi)?;
                    Ok(())
                }
            }
            CardDisplayPhase::Sleep => {
                if self.queue.len() > 0 {
                    self.timer = time_tick.clone();
                    let card = self.queue.pop_front().unwrap();
                    self.load_card_image(&card)?;

                    self.phase = CardDisplayPhase::CardBackRotateOut;
                    self.tick(time_tick, frame, frame_rect)
                } else {
                    let roi = frame.roi(self.card_rect)?;
                    let card = remove_color(&roi, &self.card_back, &GREEN)?;
                    place_umat(&card, frame, self.card_rect)?;
                    Ok(())
                }
            }
        }
    }

    pub fn load_card_image(&mut self, display_card: &DataRow) -> Result<()> {
        let mut img = self
            .card_db
            .load_card_image(&display_card.name, &display_card.pitch);
        if img.cols() > img.rows() {
            let mut rotated_card_image = UMat::new_def();
            opencv::core::rotate(
                &img,
                &mut rotated_card_image,
                opencv::core::ROTATE_90_CLOCKWISE,
            )?;
            img = rotated_card_image
        }
        opencv::imgproc::resize(
            &img.clone(),
            &mut img,
            self.card_rect.size(),
            0.0,
            0.0,
            opencv::imgproc::INTER_LINEAR,
        )?;
        self.display_card.replace(img);
        Ok(())
    }
}
