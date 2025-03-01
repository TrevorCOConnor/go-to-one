use chrono;
use csv::StringRecord;
use std::{
    collections::VecDeque,
    fs::File,
    io::{stdout, Write},
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

use lib::{
    autocomplete::autocomplete,
    card::{CardDB, CardData},
};

#[derive(Debug)]
struct TimeTick {
    sec: u64,
    milli: f64,
}

impl TimeTick {
    fn scale(&self, scalar: f64) -> Self {
        let new_milli = self.milli * scalar;
        let overflow = new_milli.div_euclid(MILLI);
        let new_sec = (self.sec as f64) * scalar + overflow;
        let new_milli = new_milli.rem_euclid(MILLI);

        TimeTick {
            sec: new_sec as u64,
            milli: new_milli,
        }
    }

    fn offset(&self, offset: f64) -> Self {
        let _offset = offset * MILLI;

        let sec_offset = _offset.div_euclid(MILLI);
        let milli_offset = _offset.rem_euclid(MILLI);

        let new_sec = (self.sec as f64) + sec_offset;
        let new_milli = self.milli + milli_offset;

        TimeTick {
            sec: new_sec as u64,
            milli: new_milli,
        }
    }
}

const MILLI: f64 = 1000.0;

fn is_command(text: &str) -> bool {
    text.starts_with(":")
}

fn life_update(text: &str) -> bool {
    text.starts_with("-1") || text.starts_with("-2")
}

async fn print_events(
    player1: &str,
    player2: &str,
    card_db: &CardDB,
    player_life: Option<(String, String)>,
) {
    let mut reader = EventStream::new();
    let mut text = String::new();
    let mut suggestions: VecDeque<&CardData> = VecDeque::new();

    let output_fp = format!(
        "annotations/{}_v_{}_{}.csv",
        player1,
        player2,
        chrono::Local::now()
    );
    let mut output_file = File::create(output_fp).expect("Couldn't write to file");

    let _ = write!(
        output_file,
        "sec\tmilli\tuuid\tname\tpitch\tplayer1_life\tplayer2_life\tupdate_type\n"
    );

    // Set starting life totals if given
    if let Some((player1, player2)) = player_life {
        let _ = write!(
            output_file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            0, 0, "", "", "", player1, player2, "life"
        );
    }

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
                                // If paused, unpause on space, enter, or esc
                                match key.code {
                                    KeyCode::Enter => {},
                                    KeyCode::Char(' ') => {}
                                    KeyCode::Esc => {}
                                    _ => {
                                        continue;
                                    }
                                }
                                paused = false;
                                offset += time::Instant::now() - start_paused_time;
                                text = String::new();
                                suggestions = VecDeque::new();
                            } else if is_command(&text) {
                                match key.code {
                                    KeyCode::Char(c) => {
                                        text.push(c);
                                    },
                                    KeyCode::Backspace => {
                                        text.pop();
                                    },
                                    KeyCode::Esc => {
                                        text = String::new();
                                        suggestions = VecDeque::new();
                                    },
                                    KeyCode::Enter => {
                                        if text.starts_with(":q") {
                                            println!();
                                            break;
                                        } else if text.starts_with(":p") {
                                            paused = true;
                                            start_paused_time = time::Instant::now();
                                        } else {
                                            let pos = position().unwrap();
                                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                            println!("> Command '{}' not recognized", text);
                                            text = String::new();
                                            suggestions = VecDeque::new();
                                        }
                                    }
                                    _ => continue
                                }
                            } else if life_update(&text) {
                                match key.code {
                                    KeyCode::Char(c) => {
                                        text.push(c);
                                    },
                                    KeyCode::Backspace => {
                                        text.pop();
                                    },
                                    KeyCode::Enter => {
                                        let t_clone = text.clone();
                                        let split: Vec<&str> = t_clone.split(" ").collect();
                                        if split.len() == 2 {
                                            let player = split[0];
                                            let life = split[1];

                                            if (player != "-1" && player != "-2") || life.parse::<i32>().is_err() {
                                                let pos = position().unwrap();
                                                let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                                println!("> Invalid life command. Should be of format '-1 30' or '-2 28'.");
                                            } else {
                                                let time_stamp = time::Instant::now() - start_time - offset;

                                                let (player1, player2) = {
                                                    if player == "-1" {
                                                        text = format!("Player 1's life set to {}", life);
                                                        (Some(life), None)
                                                    } else {
                                                        text = format!("Player 2's life set to {}", life);
                                                        (None, Some(life))
                                                    }
                                                };

                                                let _ = write!(
                                                    output_file,
                                                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                                                    time_stamp.as_secs(),
                                                    time_stamp.as_millis().rem_euclid(MILLI as u128),
                                                    "",
                                                    "",
                                                    "",
                                                    player1.unwrap_or(""),
                                                    player2.unwrap_or(""),
                                                    "life"
                                                );
                                            }

                                            let pos = position().unwrap();
                                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                            println!("> {}", text);
                                            text = String::new();
                                            suggestions = VecDeque::new();
                                        }
                                    }
                                    _ => continue
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char(c) => {
                                        text.push(c);
                                        suggestions = VecDeque::from(autocomplete(&card_db.cards, &text));
                                    },
                                    KeyCode::Backspace => {
                                        text.pop();
                                    },
                                    KeyCode::Esc => {
                                        text = String::new();
                                        suggestions = VecDeque::new()
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
                                                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                                                time_stamp.as_secs(),
                                                time_stamp.as_millis().rem_euclid(MILLI as u128),
                                                suggest.uuid,
                                                suggest.name,
                                                suggest.pitch_str(),
                                                "",
                                                "",
                                                "card"
                                            );

                                            let pos = position().unwrap();
                                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                            println!("> {}", suggest.display);
                                            text = String::new();
                                            suggestions = VecDeque::new();
                                        }
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

fn modify_time(input_fp: &str, output_fp: &str, scalar: Option<f64>, offset: Option<f64>) {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_path(input_fp)
        .expect("Could not load card file");

    let headers = reader.headers().unwrap().clone();

    let records: Vec<Result<StringRecord, _>> = reader.into_records().collect();

    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(output_fp)
        .unwrap();

    let _ = wtr.write_record(&headers);

    for record in records {
        let record = record.unwrap();
        let sec = record[0].parse::<u64>().expect("Sec invalid");
        let milli = record[1].parse::<f64>().expect("Milli invalid");

        let mut time_tick = TimeTick { sec, milli };
        if let Some(sclr) = scalar {
            time_tick = time_tick.scale(sclr);
        }
        if let Some(off) = offset {
            time_tick = time_tick.offset(off)
        }

        let mut new_line = vec![time_tick.sec.to_string(), time_tick.milli.to_string()];
        new_line.extend(record.iter().map(|v| v.to_string()).skip(2));

        let _ = wtr.write_record(&new_line);
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        println!("Player name arguments missing");
        exit(0)
    }
    if args[1] == "--modify" {
        let input_fp = &args[2];
        let output_fp = &args[3];
        let mut scalar = None;
        let mut offset = None;

        for i in 0..=1 {
            if args.len() >= 5 + 2 * i {
                let modifier = &args[4 + (2 * i)];
                println!("{}", modifier);
                if modifier == "--scale" {
                    scalar = args[5 + 2 * i].parse::<f64>().ok();
                }
                if modifier == "--offset" {
                    offset = args[5 + 2 * i].parse::<f64>().ok();
                }
            }
        }
        modify_time(input_fp, output_fp, scalar, offset);
        exit(0);
    }

    let player1 = args[1].to_string();
    let player2 = args[2].to_string();
    let mut player_life = None;
    loop {
        println!("Enter starting life for both heroes or press enter to use default values:");
        let mut input: String = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();
        if input.is_empty() {
            break;
        }

        let split = input.split(" ").collect::<Vec<&str>>();
        if split.len() == 2 {
            let life1 = split[0];
            let life2 = split[1];
            if life1.parse::<u32>().is_ok() && life2.parse::<u32>().is_ok() {
                let _ = player_life.insert((life1.to_string(), life2.to_string()));
                break;
            }
        }

        println!("Invalid input.");
    }
    println!("Timer started!");
    print!("> ");

    enable_raw_mode()?;

    let mut stdout = stdout();
    execute!(stdout)?;

    let card_db = CardDB::init();

    print_events(&player1, &player2, &card_db, player_life).await;

    execute!(stdout)?;

    disable_raw_mode()
}
