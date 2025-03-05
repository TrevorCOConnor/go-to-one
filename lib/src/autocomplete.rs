use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent};

pub trait Named {
    fn get_name(&self) -> &str;
}

pub fn autocomplete<'a, T: Named>(values: &'a [T], text: &str) -> Vec<&'a T> {
    values
        .iter()
        .filter_map(|item| {
            if item
                .get_name()
                .to_lowercase()
                .starts_with(&text.to_lowercase())
            {
                Some(item)
            } else {
                None
            }
        })
        .collect()
}

pub enum AutocompleteResult<'a, T: Named> {
    Continue {
        text: String,
        suggestions: VecDeque<&'a T>,
    },
    Finished(&'a T),
}

pub fn get_user_input_for_autocomplete<'a, T: Named>(
    values: &'a [T],
    current_text: &str,
    current_suggestions: &VecDeque<&'a T>,
    key: KeyEvent,
) -> AutocompleteResult<'a, T> {
    let mut new_text = current_text.to_string();
    let mut new_suggestions = current_suggestions.clone();
    match key.code {
        KeyCode::Char(c) => {
            // Add character to current text and update suggestions
            new_text.push(c);
            new_suggestions = VecDeque::from(autocomplete(values, &new_text));

            // Ignore character if no matches
            if new_suggestions.len() == 0 {
                new_text = current_text.to_owned();
                new_suggestions = current_suggestions.clone();
            }
        }
        KeyCode::Backspace => {
            new_text.pop();
        }
        KeyCode::Esc => {
            new_text = String::new();
            new_suggestions = VecDeque::new()
        }
        KeyCode::BackTab => {
            if new_suggestions.len() > 0 {
                new_suggestions.rotate_right(1);
            }
        }
        KeyCode::Tab => {
            if new_suggestions.len() > 0 {
                new_suggestions.rotate_left(1);
            }
        }
        KeyCode::Enter => {
            if let Some(suggest) = new_suggestions.front() {
                return AutocompleteResult::Finished(suggest);
            }
        }
        _ => {}
    };
    AutocompleteResult::Continue {
        text: new_text,
        suggestions: new_suggestions,
    }
}

pub struct AutocompleteOption(String);

impl AutocompleteOption {
    pub fn new(option: String) -> Self {
        AutocompleteOption(option)
    }

    pub fn text(&self) -> &str {
        &self.0
    }
}

impl Named for AutocompleteOption {
    fn get_name(&self) -> &str {
        &self.0
    }
}

impl Named for &AutocompleteOption {
    fn get_name(&self) -> &str {
        &self.0
    }
}
