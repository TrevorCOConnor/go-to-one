use lib::{image::FullArtHeroManager, intro::{VideoCapLooper, VideoCapLooperAdj}, relative_roi::RelativeRoi};
use opencv::{core::{flip, UMat, UMatTrait, UMatTraitConst}, imgproc};

use crate::{TurnPlayer, HERO_BORDER_THICKNESS, HERO_DEF_COLOR, HERO_TURN_COLOR, HERO_WIN_COLOR};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;


pub struct DisplayHeroManager{
    hero1_loop: VideoCapLooperAdj,
    hero2_loop: VideoCapLooperAdj,
}

impl DisplayHeroManager {
    pub fn new(hero1_name: &str, hero2_name: &str, _time_modifier: f64) -> Result<Self> {
        let full_art_manager = FullArtHeroManager::new();

        let hero1_animation_fp = full_art_manager.get_hero_art_animation_fp(&hero1_name)?;
        let hero2_animation_fp = full_art_manager.get_hero_art_animation_fp(&hero2_name)?;

        Ok(Self {
            hero1_loop: VideoCapLooperAdj::build(&hero1_animation_fp)?,
            hero2_loop: VideoCapLooperAdj::build(&hero2_animation_fp)?,
        })
    }

    pub fn new_def(hero1_name: &str, hero2_name: &str) -> Result<Self> {
        Self::new(hero1_name, hero2_name, 1.0)
    }

    pub fn display_heroes(
        &mut self,
        frame: &mut UMat,
        hero1_rel_roi: RelativeRoi,
        hero2_rel_roi: RelativeRoi,
        turn_player: &TurnPlayer,
        winner: Option<u8>,
    ) -> Result<()> {
        // frame size
        let frame_size = frame.size()?;

        // Heroes
        let hero1_image = self.hero1_loop.read()?;
        let mut hero1_image = FullArtHeroManager::crop_hero_img(&hero1_image)?;
        flip(&hero1_image.clone(), &mut hero1_image, 1)?;
        let hero1_rect = hero1_rel_roi.generate_roi(&frame_size, &hero1_image);
        let hero1_image = hero1_rel_roi.resize(&frame_size, &hero1_image)?;

        let mut hero1_roi = frame.roi_mut(hero1_rect)?;
        hero1_image.copy_to(&mut hero1_roi)?;
        let hero1_color = {
            if winner.is_some_and(|v| v == 1) {
                HERO_WIN_COLOR
            } else if *turn_player == TurnPlayer::One {
                HERO_TURN_COLOR
            } else {
                HERO_DEF_COLOR
            }
        };
        imgproc::rectangle(
            frame,
            hero1_rect,
            hero1_color,
            HERO_BORDER_THICKNESS,
            imgproc::LINE_8,
            0,
        )?;

        let hero2_image = self.hero2_loop.read()?;
        let hero2_image = FullArtHeroManager::crop_hero_img(&hero2_image)?;
        let hero2_rect = hero2_rel_roi.generate_roi(&frame_size, &hero2_image);
        let hero2_image = hero2_rel_roi.resize(&frame_size, &hero2_image)?;

        let mut hero2_roi = frame.roi_mut(hero2_rect)?;
        hero2_image.copy_to(&mut hero2_roi)?;

        let hero2_color = {
            if winner.is_some_and(|v| v == 2) {
                HERO_WIN_COLOR
            } else if *turn_player == TurnPlayer::Two {
                HERO_TURN_COLOR
            } else {
                HERO_DEF_COLOR
            }
        };
        imgproc::rectangle(
            frame,
            hero2_rect,
            hero2_color,
            HERO_BORDER_THICKNESS,
            imgproc::LINE_8,
            0,
        )?;
        Ok(())
    }
}
