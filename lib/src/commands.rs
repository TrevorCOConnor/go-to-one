use std::{collections::VecDeque, io::stdout};

use crossterm::{
    cursor::{position, MoveTo, MoveUp},
    event::{Event, EventStream},
    execute,
    style::Stylize,
    terminal::{Clear, ClearType},
};

use futures::{future::FutureExt, select, StreamExt};

use crate::{
    autocomplete::{get_user_input_for_autocomplete, AutocompleteResult},
    card::CardData,
};

pub async fn enter_card<'a>(cards: &[&'a CardData]) -> &'a CardData {
    let mut reader = EventStream::new();
    let mut text = String::new();
    let mut suggestions = VecDeque::new();
    println!("> ");
    let _ = execute!(stdout(), MoveUp(1));
    loop {
        let mut event = reader.next().fuse();
        select! {
            maybe_event = event => {
                match maybe_event {
                    Some(Ok(event)) => {
                        if let Event::Key(key) = event {
                            let res = get_user_input_for_autocomplete(cards, &text, &suggestions, key);
                            match res {
                                AutocompleteResult::Finished(card) => {
                                    let pos = position().unwrap();
                                    let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                    println!("> {}", card.display);
                                    let pos = position().unwrap();
                                    let _  = execute!(stdout(), MoveTo(0, pos.1));
                                    return card;
                                }
                                AutocompleteResult::Continue{text: new_text, suggestions: new_suggestions} => {
                                    text = new_text;
                                    suggestions = new_suggestions;
                                }
                            }
                        }
                        let pos = position().unwrap();
                        let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));

                        let display = {
                            if let Some(suggest) = suggestions.front() {
                                let split = suggest.display.split_at(text.len());
                                &format!("{}{}", split.0, split.1.grey())
                            } else {
                                &text
                            }
                        };
                        println!("> {}", display);
                        let _ = execute!(stdout(), MoveUp(1));
                    },
                    Some(Err(e)) => println!("Error: {:?}\r", e),
                    None => {},
                }
            }
        }
    }
}
