use opencv::{
    core::{bitwise_not_def, flip, Rect, Scalar, Size, UMat, UMatTraitConst},
    imgproc::{cvt_color_def, resize_def, COLOR_BGR2GRAY, COLOR_GRAY2RGB, FONT_HERSHEY_SIMPLEX},
    videoio::{VideoCapture, VideoCaptureTrait, VideoWriter, VideoWriterTrait},
};

use crate::{
    image::crop,
    movement::{place_umat, Reparameterization},
    relative_roi::center_offset,
    text::center_text_at_rect,
};

pub const INTRO_TIME: f64 = 8.0;
const PLAYER_NAME_FONT_SCALE: f64 = 4.0;
const PLAYER_NAME_FONT_FACE: i32 = FONT_HERSHEY_SIMPLEX;
const PLAYER_NAME_FONT_THICKNESS: i32 = 6;
const PLAYER_NAME_FONT_BUFFER: i32 = 20;
const WHITE: Scalar = Scalar::new(255.0, 255.0, 255.0, 0.0);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct VideoCapLooper {
    fp: String,
    cap: VideoCapture,
}

impl VideoCapLooper {
    pub fn build(video_fp: &str) -> Result<Self> {
        let cap = VideoCapture::from_file_def(video_fp)?;
        Ok(Self {
            fp: video_fp.to_owned(),
            cap,
        })
    }

    pub fn read(&mut self) -> Result<UMat> {
        let mut frame = UMat::new_def();
        let got = self.cap.read(&mut frame)?;
        if !got {
            self.cap = VideoCapture::from_file_def(&self.fp)?;
            self.cap.read(&mut frame)?;
        }

        Ok(frame)
    }

    // Hack until I make this file myself
    pub fn background_read(&mut self) -> Result<UMat> {
        let mut frame = UMat::new_def();
        let got = self.cap.read(&mut frame)?;
        if !got {
            self.cap = VideoCapture::from_file_def(&self.fp)?;
            self.cap.read(&mut frame)?;
        }

        // HACK
        let mut inverted_frame = UMat::new_def();
        bitwise_not_def(&frame, &mut inverted_frame)?;
        cvt_color_def(&inverted_frame, &mut frame, COLOR_BGR2GRAY)?;
        // cvt_color_def(&frame, &mut inverted_frame, COLOR_RGB2BGR)?;
        cvt_color_def(&frame, &mut inverted_frame, COLOR_GRAY2RGB)?;

        Ok(inverted_frame)
    }
}

fn bounce_in(percentage: f64, img: &UMat, frame: &mut UMat, right: bool) -> Result<()> {
    let frame_size = frame.size()?;

    let crop_point = Reparameterization::Bounce.apply(percentage) * frame_size.width as f64;
    let crop_x = frame_size.width - (crop_point as i32);

    let crop_rect = {
        if right {
            Rect::new(
                crop_x,
                0,
                frame_size.width - crop_x,
                frame_size.height.div_euclid(2),
            )
        } else {
            Rect::new(
                0,
                0,
                frame_size.width - crop_x,
                frame_size.height.div_euclid(2),
            )
        }
    };

    let cropped = crop(&img, &crop_rect)?;

    let place_rect = {
        if right {
            Rect::new(0, 0, cropped.size()?.width, cropped.size()?.height)
        } else {
            Rect::new(
                crop_x,
                frame_size.height.div_euclid(2),
                cropped.size()?.width,
                cropped.size()?.height,
            )
        }
    };
    place_umat(&cropped, frame, place_rect)?;
    Ok(())
}

pub fn generate_intro(
    hero1_fp: &str,
    player1: &str,
    hero2_fp: &str,
    player2: &str,
    frame_size: &Size,
    frame_typ: i32,
    fps: f64,
    writer: &mut VideoWriter,
) -> Result<()> {
    let num_frames = (fps * (INTRO_TIME / 4.0)) as i32;
    let img_size = Size::new(frame_size.width, frame_size.height.div_euclid(2));
    let mut hero1_looper = VideoCapLooper::build(hero1_fp)?;
    let mut hero2_looper = VideoCapLooper::build(hero2_fp)?;

    let mut hero1_img = hero1_looper.read()?;
    let mut hero2_img = hero2_looper.read()?;
    flip(&hero1_img.clone(), &mut hero1_img, 1)?;
    resize_def(&hero1_img.clone(), &mut hero1_img, img_size)?;
    resize_def(&hero2_img.clone(), &mut hero2_img, img_size)?;

    for i in 0..num_frames {
        let mut frame = UMat::new_size_with_default_def(
            *frame_size,
            frame_typ,
            Scalar::new(0.0, 0.0, 0.0, 0.0),
        )?;

        let percentage = i as f64 / num_frames as f64;
        bounce_in(percentage, &hero1_img, &mut frame, true)?;
        writer.write(&frame)?;
    }
    for i in 0..num_frames {
        // Place first image
        let mut frame = UMat::new_size_with_default_def(
            *frame_size,
            frame_typ,
            Scalar::new(0.0, 0.0, 0.0, 0.0),
        )?;
        let mut hero1_img = hero1_looper.read()?;
        flip(&hero1_img.clone(), &mut hero1_img, 1)?;
        resize_def(&hero1_img.clone(), &mut hero1_img, img_size)?;
        place_umat(
            &hero1_img,
            &mut frame,
            Rect::new(0, 0, img_size.width, img_size.height),
        )?;

        let percentage = i as f64 / num_frames as f64;
        bounce_in(percentage, &hero2_img, &mut frame, false)?;

        writer.write(&frame)?;
    }
    for i in 0..(2 * num_frames) {
        let mut frame = UMat::new_size_with_default_def(
            *frame_size,
            frame_typ,
            Scalar::new(0.0, 0.0, 0.0, 0.0),
        )?;
        let mut hero1_img = hero1_looper.read()?;
        flip(&hero1_img.clone(), &mut hero1_img, 1)?;
        resize_def(&hero1_img.clone(), &mut hero1_img, img_size)?;
        place_umat(
            &hero1_img,
            &mut frame,
            Rect::new(0, 0, img_size.width, img_size.height),
        )?;

        let mut hero2_img = hero2_looper.read()?;
        resize_def(&hero2_img.clone(), &mut hero2_img, img_size)?;
        place_umat(
            &hero2_img,
            &mut frame,
            Rect::new(
                0,
                frame_size.height.div_euclid(2),
                img_size.width,
                img_size.height,
            ),
        )?;

        if i > num_frames {
            center_text_at_rect(
                &mut frame,
                player1,
                PLAYER_NAME_FONT_FACE,
                PLAYER_NAME_FONT_SCALE,
                WHITE,
                PLAYER_NAME_FONT_THICKNESS,
                Rect::new(
                    center_offset(3 * img_size.width.div_euclid(5), img_size.width),
                    center_offset(3 * img_size.height.div_euclid(5), img_size.height),
                    3 * img_size.width.div_euclid(5),
                    3 * img_size.height.div_euclid(5),
                ),
                PLAYER_NAME_FONT_BUFFER,
            )?;
            center_text_at_rect(
                &mut frame,
                player2,
                PLAYER_NAME_FONT_FACE,
                PLAYER_NAME_FONT_SCALE,
                WHITE,
                PLAYER_NAME_FONT_THICKNESS,
                Rect::new(
                    center_offset(3 * img_size.width.div_euclid(5), img_size.width),
                    frame_size.height.div_euclid(2)
                        + center_offset(3 * img_size.height.div_euclid(5), img_size.height),
                    3 * img_size.width.div_euclid(5),
                    3 * img_size.height.div_euclid(5),
                ),
                PLAYER_NAME_FONT_BUFFER,
            )?;
            center_text_at_rect(
                &mut frame,
                "VS",
                PLAYER_NAME_FONT_FACE,
                PLAYER_NAME_FONT_SCALE,
                WHITE,
                PLAYER_NAME_FONT_THICKNESS,
                Rect::new(
                    center_offset(1 * frame_size.width.div_euclid(5), frame_size.width),
                    center_offset(1 * frame_size.height.div_euclid(5), frame_size.height),
                    1 * frame_size.width.div_euclid(5),
                    1 * frame_size.height.div_euclid(5),
                ),
                PLAYER_NAME_FONT_BUFFER,
            )?;
        }
        writer.write(&frame)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use opencv::{
        core::{Size, CV_8UC3},
        videoio::VideoWriter,
    };

    use super::generate_intro;

    #[test]
    fn test_intro() -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = VideoWriter::new_def(
            "data/test/intro_test.mp4",
            VideoWriter::fourcc('a', 'v', 'c', '1').unwrap(),
            60.0,
            Size::new(1920, 1080),
        )?;
        let frame_size = Size::new(1920, 1080);
        let frame_type = CV_8UC3;
        let fps = 60.0;

        let hero1_fp = std::env::current_dir()?
            .parent()
            .unwrap()
            .join("data/full_art_heroes/rhinar.mp4");
        let hero1_fp = hero1_fp.to_str().unwrap();

        let hero2_fp = std::env::current_dir()?
            .parent()
            .unwrap()
            .join("data/full_art_heroes/maxx.mp4");
        let hero2_fp = hero2_fp.to_str().unwrap();

        generate_intro(
            &hero1_fp,
            "Tom",
            &hero2_fp,
            "Trevor",
            &frame_size,
            frame_type,
            fps,
            &mut writer,
        )?;
        Ok(())
    }
}
