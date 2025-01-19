mod autocomplete;
mod card_db;

use chrono;
use std::{
    collections::VecDeque,
    fs::File,
    io::{stdout, Read, Write},
    process::exit,
    time::{self, Duration},
};

use futures::{future::FutureExt, select, StreamExt};

use crossterm::{
    cursor::{position, MoveTo, MoveUp},
    event::{Event, EventStream, KeyCode},
    execute,
    style::Stylize,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};

use crate::{
    autocomplete::autocomplete_card_name,
    card_db::{CardDB, CardData},
};

pub async fn print_events(player1: &str, player2: &str, card_db: &CardDB) {
    let mut reader = EventStream::new();
    let mut text = String::new();
    let mut suggestions: VecDeque<&CardData> = VecDeque::new();

    let output_fp = format!("{}_v_{}_{}.csv", player1, player2, chrono::Local::now());
    let mut output_file = File::create(output_fp).expect("Couldn't write to file");

    let _ = write!(output_file, "sec\tmilli\tuuid\tname\tpitch\n");

    let start_time = time::Instant::now();
    let mut offset = Duration::from_secs(0);
    let mut paused = false;
    let mut start_paused_time = time::Instant::now();

    loop {
        let mut event = reader.next().fuse();

        select! {
            maybe_event = event => {
                match maybe_event {
                    Some(Ok(event)) => {
                        if let Event::Key(key) = event {
                            if paused {
                                match key.code {
                                    KeyCode::Home => {
                                        paused = false;
                                        offset += time::Instant::now() - start_paused_time;
                                    }
                                    _ => {
                                        continue;
                                    }
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char(c) => {
                                        text.push(c);
                                        suggestions = VecDeque::from(autocomplete_card_name(card_db, &text));
                                    },
                                    KeyCode::Backspace => {
                                        text.pop();
                                    },
                                    KeyCode::Esc => {
                                        break;
                                    },
                                    KeyCode::BackTab => {
                                        suggestions.rotate_right(1);
                                    },
                                    KeyCode::Tab => {
                                        suggestions.rotate_left(1);
                                    },
                                    KeyCode::Enter => {
                                        if let Some(suggest) = suggestions.front() {
                                            let time_stamp = time::Instant::now() - start_time - offset;
                                            let _ = write!(
                                                output_file,
                                                "{}\t{}\t{}\t{}\t{}\n",
                                                time_stamp.as_secs(),
                                                time_stamp.as_millis(),
                                                suggest.uuid,
                                                suggest.name,
                                                suggest.pitch_str()
                                            );

                                            let pos = position().unwrap();
                                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                            println!("> {}", suggest.display);
                                            text = String::new();
                                            suggestions = VecDeque::new();
                                        }
                                    }
                                    KeyCode::Home => {
                                        paused = true;
                                        start_paused_time = time::Instant::now();
                                    }
                                    _ => continue
                                }
                            }
                            let pos = position().unwrap();
                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));

                            let display = {
                                if paused {
                                    "PAUSED"
                                } else {
                                    if let Some(suggest) = suggestions.front() {
                                        let split = suggest.display.split_at(text.len());
                                        &format!("{}{}", split.0, split.1.grey())
                                    } else {
                                        &text
                                    }
                                }
                            };
                            println!("> {}", display);
                            let _ = execute!(stdout(), MoveUp(1));
                        }
                    }
                    Some(Err(e)) => println!("Error: {:?}\r", e),
                    None => break,
                }
            }
        };
    }
}

#[tokio::main]
pub async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Player name arguments missing");
        exit(0)
    }
    let player1 = args[1].to_string();
    let player2 = args[2].to_string();
    println!("Press Enter to begin:");
    let mut input: String = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    println!("Timer started!");
    print!("> ");

    enable_raw_mode()?;

    let mut stdout = stdout();
    execute!(stdout)?;

    let card_db = CardDB::init();

    print_events(&player1, &player2, &card_db).await;

    execute!(stdout)?;

    disable_raw_mode()
}
