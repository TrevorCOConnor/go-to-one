use csv::StringRecord;
use log::warn;
use opencv::{
    core::{MatTraitConst, UMat, UMatTrait, UMatTraitConst, Vector, CV_8U},
    imgcodecs::{imdecode, IMREAD_UNCHANGED},
    imgproc::{cvt_color_def, COLOR_RGBA2RGB},
    rgbd::rescale_depth_def,
};

/// This may need to be replaced with an actual DB at some point
use std::{collections::HashMap, fs::File};

use crate::{autocomplete::Named, fade::convert_alpha_to_white};

const URL_FILE: &'static str = "data/card_data.csv";
const CARD_FILE: &'static str = "data/card.csv";

#[derive(Debug, Clone)]
pub struct CardData {
    pub name: String,
    pub pitch: Option<u32>,
    pub life: Option<u32>,
    pub display: String,
    pub uuid: String,
    pub types: Vec<String>,
}

impl CardData {
    pub fn pitch_str(&self) -> String {
        self.pitch.map(|v| v.to_string()).unwrap_or("".to_string())
    }

    fn build_from_record(headers: &HashMap<String, usize>, record: StringRecord) -> Option<Self> {
        if !headers.contains_key("Name") {
            warn!("Card file missing key {}", "Name");
            return None;
        }
        let name = record[headers["Name"]].to_owned();
        let pitch = match &record[headers["Pitch"]] {
            "1" => " (R)".to_string(),
            "2" => " (Y)".to_string(),
            "3" => " (B)".to_string(),
            _ => "".to_string(),
        };
        Some(CardData {
            name: name.clone(),
            pitch: record[headers["Pitch"]].parse::<u32>().ok(),
            life: record[headers["Health"]].parse::<u32>().ok(),
            display: format!("{}{}", name, pitch),
            uuid: record[headers["Unique ID"]].to_string(),
            types: record
                .get(headers["Types"])
                .unwrap_or("")
                .to_string()
                .split(",")
                .map(|v| v.trim().to_lowercase())
                .collect(),
        })
    }
}

pub struct CardDB {
    pub cards: Vec<CardData>,
}

impl CardDB {
    pub fn init() -> Self {
        // Load card data
        let file = File::open(CARD_FILE).expect(&format!("Could not find {}", CARD_FILE));
        let mut reader = csv::ReaderBuilder::new().delimiter(b'\t').from_reader(file);
        let headers = reader.headers().expect("Headers not found").to_owned();
        let headers =
            HashMap::from_iter(headers.iter().enumerate().map(|(e, v)| (v.to_owned(), e)));

        let mut cards = Vec::new();
        for record in reader.records() {
            let rec = record.expect("Broken record found.");
            // Add url data from other file
            if let Some(new_card) = CardData::build_from_record(&headers, rec) {
                cards.push(new_card);
            }
        }
        CardDB { cards }
    }

    pub fn heroes(&self) -> Vec<&CardData> {
        self.cards
            .iter()
            .filter(|c| c.types.contains(&"hero".to_string()))
            .collect()
    }

    pub fn find(&self, name: &str, pitch: Option<u32>) -> Option<&CardData> {
        self.cards
            .iter()
            .filter(|c| c.name == name && c.pitch == pitch)
            .next()
    }
}

impl Named for CardData {
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl Named for &CardData {
    fn get_name(&self) -> &str {
        &self.name
    }
}

pub struct CardImageDB {
    uuid_card_map: HashMap<(String, Option<u32>), String>,
}

impl CardImageDB {
    pub fn init() -> Self {
        let mut map: HashMap<(String, Option<u32>), String> = HashMap::new();
        let file = File::open(URL_FILE).expect(&format!("Could not find {}", URL_FILE));

        let mut reader = csv::ReaderBuilder::new().delimiter(b'\t').from_reader(file);
        let headers = reader.headers().expect("Headers not found").to_owned();
        let headers: HashMap<String, usize> =
            HashMap::from_iter(headers.iter().enumerate().map(|(e, v)| (v.to_owned(), e)));

        for row in reader.into_records() {
            let row = row.unwrap();
            let name = row[headers["Card Name"]].to_string();
            let set = row[headers["Set ID"]].to_string();
            let pitch = row[headers["Card Pitch"]].parse::<u32>().ok();
            // Skip HP1 and promo cards
            if ["HP1", "FAB", "HER", "WIN"].contains(&&set[0..=2]) {
                continue;
            }
            let art_variations = row[headers["Art Variations"]].to_string();
            // Skip art variations if possible
            // Some cards only have art variations, though
            if !art_variations.trim().is_empty() && map.contains_key(&(name.clone(), pitch)) {
                continue;
            }
            map.insert((name, pitch), row[headers["Image URL"]].to_string());
        }

        Self { uuid_card_map: map }
    }

    pub fn load_card_image(&self, name: &str, pitch: &Option<u32>) -> UMat {
        let key = (name.to_string(), pitch.to_owned());
        let url = self
            .uuid_card_map
            .get(&key)
            .expect(&format!("{:?} not found in card image db", key));

        let mut image_mat = UMat::new(opencv::core::UMatUsageFlags::USAGE_DEFAULT);
        let img_vec = reqwest::blocking::get(url)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec();
        let img_vec: Vector<u8> = Vector::from_iter(img_vec);
        let img = imdecode(&img_vec, IMREAD_UNCHANGED).unwrap();

        img.copy_to(&mut image_mat).unwrap();

        let img = convert_alpha_to_white(&image_mat).unwrap();
        cvt_color_def(&img, &mut image_mat, COLOR_RGBA2RGB).unwrap();

        // I don't totally understand this, but Splatter Skull had a depth of 2 whereas every other
        // image has a depth of 0, so this catches that case
        if image_mat.depth() > 0 {
            opencv::prelude::UMatTraitConst::convert_to_def(
                &image_mat.clone(),
                &mut image_mat,
                CV_8U,
            )
            .unwrap();
        }

        return image_mat;
    }
}
