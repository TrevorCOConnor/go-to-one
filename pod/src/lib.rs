use std::collections::HashMap;
use std::fs::File;

const CARD_DATA_FP: &str = "data/card_data.csv";

pub struct CardDB {
    uuid_card_map: HashMap<(String, Option<u32>), String>,
}

impl CardDB {
    pub fn init() -> Self {
        let mut map: HashMap<(String, Option<u32>), String> = HashMap::new();
        let file = File::open(CARD_DATA_FP).expect("Could not find card.csv");

        let mut reader = csv::ReaderBuilder::new().delimiter(b'\t').from_reader(file);
        let headers = reader.headers().expect("Headers not found").to_owned();
        let headers: HashMap<String, usize> =
            HashMap::from_iter(headers.iter().enumerate().map(|(e, v)| (v.to_owned(), e)));

        for row in reader.into_records() {
            let row = row.unwrap();
            let name = row[headers["Card Name"]].to_string();
            let set = row[headers["Set ID"]].to_string();
            if set.starts_with("1HP") || set.starts_with("FAB") {
                continue;
            }
            let art_variations = row[headers["Art Variations"]].to_string();
            if !art_variations.trim().is_empty() {
                continue;
            }
            let pitch = row[headers["Card Pitch"]].parse::<u32>().ok();
            map.insert((name, pitch), row[headers["Image URL"]].to_string());
        }

        CardDB { uuid_card_map: map }
    }

    pub fn load_card_image(&self, name: &str, pitch: &Option<u32>, fp: &str) {
        let url = &self.uuid_card_map[&(name.to_string(), pitch.to_owned())];

        let mut file = std::fs::File::create(fp).unwrap();
        reqwest::blocking::get(url)
            .unwrap()
            .copy_to(&mut file)
            .unwrap();
    }
}
