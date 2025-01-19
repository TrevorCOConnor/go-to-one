use csv::StringRecord;
use log::warn;

/// This may need to be replaced with an actual DB at some point
use std::{collections::HashMap, fs::File};

#[derive(Debug)]
pub struct CardData {
    pub name: String,
    pub pitch: Option<u32>,
    pub display: String,
    pub uuid: String,
}

impl CardData {
    pub fn pitch_str(&self) -> String {
        self.pitch.map(|v| v.to_string()).unwrap_or("".to_string())
    }
}

impl CardData {
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
            display: format!("{}{}", name, pitch),
            uuid: record[headers["Unique ID"]].to_string(),
        })
    }
}

pub struct CardDB {
    pub cards: Vec<CardData>,
}

impl CardDB {
    pub fn init() -> Self {
        let file = File::open("data/card.csv").expect("Could not find card.csv");
        let mut reader = csv::ReaderBuilder::new().delimiter(b'\t').from_reader(file);
        let headers = reader.headers().expect("Headers not found").to_owned();
        let headers =
            HashMap::from_iter(headers.iter().enumerate().map(|(e, v)| (v.to_owned(), e)));

        let mut cards = Vec::new();
        for record in reader.records() {
            let rec = record.expect("Broken record found.");
            if let Some(new_card) = CardData::build_from_record(&headers, rec) {
                cards.push(new_card);
            }
        }
        CardDB { cards }
    }
}
