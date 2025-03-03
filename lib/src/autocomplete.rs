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
            new_text.push(c);
            new_suggestions = VecDeque::from(autocomplete(values, &new_text));
        }
        KeyCode::Backspace => {
            new_text.pop();
        }
        KeyCode::Esc => {
            new_text = String::new();
            new_suggestions = VecDeque::new()
        }
        KeyCode::BackTab => {
            new_suggestions.rotate_right(1);
        }
        KeyCode::Tab => {
            new_suggestions.rotate_left(1);
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
