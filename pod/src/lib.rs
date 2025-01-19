use std::collections::HashMap;
use std::fs::File;

const CARD_DATA_FP: &str = "data/card_data.csv";

pub struct CardDB {
    uuid_card_map: HashMap<String, String>,
}

impl CardDB {
    pub fn init() -> Self {
        let mut map: HashMap<String, String> = HashMap::new();
        let file = File::open(CARD_DATA_FP).expect("Could not find card.csv");

        let mut reader = csv::ReaderBuilder::new().delimiter(b'\t').from_reader(file);
        let headers = reader.headers().expect("Headers not found").to_owned();
        let headers: HashMap<String, usize> =
            HashMap::from_iter(headers.iter().enumerate().map(|(e, v)| (v.to_owned(), e)));

        for row in reader.into_records() {
            let row = row.unwrap();
            map.insert(
                row[headers["Card Unique ID"]].to_string(),
                row[headers["Image URL"]].to_string(),
            );
        }

        CardDB { uuid_card_map: map }
    }

    pub fn load_card_image(&self, uuid: &str, fp: &str) {
        let url = &self.uuid_card_map[uuid];
        let mut file = std::fs::File::create(fp).unwrap();
        reqwest::blocking::get(url)
            .unwrap()
            .copy_to(&mut file)
            .unwrap();
    }
}
