use crate::card_db::{CardDB, CardData};

pub fn autocomplete_card_name<'a>(card_db: &'a CardDB, text: &str) -> Vec<&'a CardData> {
    card_db
        .cards
        .iter()
        .filter_map(|c| {
            if c.display.to_lowercase().starts_with(&text.to_lowercase()) {
                Some(c)
            } else {
                None
            }
        })
        .collect()
}
