use chrono;
use libmpv::{FileState, Mpv};
use std::{
    collections::VecDeque,
    fs::File,
    io::{stdout, Write},
    process::exit,
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
    autocomplete::{get_user_input_for_autocomplete, AutocompleteResult, Named},
    card::{CardDB, CardData},
    life_tracker::LifeTracker,
};

const MILLI: f64 = 1000.0;
const SEEK_SECS: f64 = 2.0;

enum Command {
    HEALTH,
    TURN,
    QUIT,
}

impl Command {
    fn get_all() -> Vec<Self> {
        Vec::from([Command::HEALTH, Command::TURN, Command::QUIT])
    }
}

impl Named for Command {
    fn get_name(&self) -> &str {
        match self {
            Command::HEALTH => ":h",
            Command::TURN => ":t",
            Command::QUIT => ":q",
        }
    }
}

fn is_command(text: &str) -> bool {
    text.starts_with(":")
}

fn is_life_update(text: &str) -> bool {
    text.starts_with(":h")
}

fn extract_life_update(text: &str) -> Option<(u8, String)> {
    let mut player = None;
    let mut update = None;

    let splits: Vec<&str> = text.split(" ").filter(|v| !v.is_empty()).collect();
    if splits.len() >= 3 && splits.first() == Some(&&":h") {
        // Parse player
        if splits[1] == "1" {
            player.replace(1);
        }
        if splits[1] == "2" {
            player.replace(2);
        }

        // Parse update
        if LifeTracker::parse_update(splits[2]).is_ok() {
            update.replace(splits[2]);
        }
    }
    if player.is_some() && update.is_some() {
        return Some((player.unwrap(), update.unwrap().to_owned()));
    }
    return None;
}

enum UpdateType {
    Life,
    Card,
    Hero1,
    Hero2,
    Turn,
}

impl UpdateType {
    fn text(&self) -> String {
        match self {
            UpdateType::Card => "card".to_string(),
            UpdateType::Life => "life".to_string(),
            UpdateType::Turn => "turn".to_string(),
            UpdateType::Hero1 => "hero1".to_string(),
            UpdateType::Hero2 => "hero2".to_string(),
        }
    }
}

struct Record {
    sec: u64,
    milli: u128,
    name: Option<String>,
    pitch: Option<u32>,
    player1_life: Option<String>,
    player2_life: Option<String>,
    update_type: UpdateType,
}

impl Record {
    fn headers() -> String {
        "sec\tmilli\tname\tpitch\tplayer1_life\tplayer2_life\tupdate_type\n".to_string()
    }

    fn text(self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.sec,
            self.milli,
            self.name.unwrap_or("".to_string()),
            self.pitch.map_or("".to_string(), |v| v.to_string()),
            self.player1_life.unwrap_or("".to_string()),
            self.player2_life.unwrap_or("".to_string()),
            self.update_type.text()
        )
    }
}

struct RecordKeeper {
    records: Vec<Record>,
}

impl RecordKeeper {
    fn build(hero1: &CardData, hero2: &CardData, first: &str) -> RecordKeeper {
        let mut rk = RecordKeeper {
            records: Vec::new(),
        };

        let hero1_record = Record {
            sec: 0,
            milli: 0,
            name: Some(hero1.name.to_owned()),
            pitch: None,
            player1_life: Some(hero1.life.unwrap().to_string()),
            player2_life: None,
            update_type: UpdateType::Hero1,
        };
        let hero2_record = Record {
            sec: 0,
            milli: 0,
            name: Some(hero2.name.to_owned()),
            pitch: None,
            player1_life: None,
            player2_life: Some(hero2.life.unwrap().to_string()),
            update_type: UpdateType::Hero2,
        };
        if first == "1" {
            rk.records.push(hero1_record);
            rk.records.push(hero2_record);
        } else {
            rk.records.push(hero1_record);
            rk.records.push(hero2_record);
        }

        rk
    }

    fn add_card_update(&mut self, mpv: &Mpv, name: &str, pitch: Option<u32>) {
        let (sec, milli) = Self::get_time(mpv);
        self.records.push(Record {
            sec,
            milli,
            name: Some(name.to_owned()),
            pitch,
            player1_life: None,
            player2_life: None,
            update_type: UpdateType::Card,
        });
    }

    fn get_time(mpv: &Mpv) -> (u64, u128) {
        let timestamp = mpv.get_property::<f64>("playback-time").unwrap();
        let sec = timestamp.trunc() as u64;
        let milli = (timestamp.fract() * MILLI) as u128;
        (sec, milli)
    }

    fn add_player_life_update(&mut self, mpv: &Mpv, player: u8, update: &str) {
        let (sec, milli) = Self::get_time(mpv);
        // Save record
        let player1_new_life = if player == 1 {
            Some(update.to_string())
        } else {
            None
        };
        let player2_new_life = if player == 2 {
            Some(update.to_string())
        } else {
            None
        };
        let record = Record {
            sec,
            milli,
            name: None,
            pitch: None,
            player1_life: player1_new_life,
            player2_life: player2_new_life,
            update_type: UpdateType::Life,
        };
        self.records.push(record);
    }

    fn add_turn_update(&mut self, mpv: &Mpv) {
        let (sec, milli) = Self::get_time(mpv);
        let record = Record {
            sec,
            milli,
            name: None,
            pitch: None,
            player1_life: None,
            player2_life: None,
            update_type: UpdateType::Turn,
        };
        self.records.push(record);
    }

    fn sort_records(&mut self) {
        self.records.sort_by_key(|v| (v.sec, v.milli));
    }
}

async fn handle_events(
    output_fp: &str,
    mpv: &Mpv,
    card_db: &CardDB,
    hero1: &CardData,
    hero2: &CardData,
    first: &str,
) {
    let mut reader = EventStream::new();
    let mut text = String::new();
    let mut card_suggestions: VecDeque<&CardData> = VecDeque::new();
    let mut command_suggestions: VecDeque<&Command> = VecDeque::new();

    let mut output_file = File::create(output_fp).expect("Couldn't write to file");

    let commands = Command::get_all();
    let mut record_keeper = RecordKeeper::build(hero1, hero2, first);

    mpv.unpause().unwrap();
    loop {
        let mut event = reader.next().fuse();
        select! {
            maybe_event = event => {
                match maybe_event {
                    Some(Ok(event)) => {
                        if let Event::Key(key) = event {
                            // Seek back
                            if key.code == KeyCode::Left && text.is_empty() {
                                let _ = mpv.seek_backward(SEEK_SECS);
                            // Seek forward
                            } else if key.code == KeyCode::Right && text.is_empty() {
                                let _ = mpv.seek_forward(SEEK_SECS);
                            // Life update
                            } else if is_life_update(&text) {
                                command_suggestions = VecDeque::new();
                                match key.code {
                                    KeyCode::Enter => {
                                        if let Some((player, update)) = extract_life_update(&text) {
                                            record_keeper.add_player_life_update(&mpv, player, &update);
                                        }
                                        let pos = position().unwrap();
                                        let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                        println!("> Player health updated");
                                        text = String::new();
                                    },
                                    KeyCode::Char(c) => {
                                        text.push(c);
                                    },
                                    KeyCode::Backspace => {
                                        text.pop();
                                    },
                                    KeyCode::Esc => {
                                        text = String::new();
                                        command_suggestions = VecDeque::new();
                                        card_suggestions = VecDeque::new();
                                    },
                                    _ => continue
                                }
                            // Command
                            } else if is_command(&text) || (text.is_empty() && key.code == KeyCode::Char(':')) {
                                let autocomplete_result = get_user_input_for_autocomplete(&commands, &text, &command_suggestions, key);

                                match autocomplete_result {
                                    AutocompleteResult::Continue{
                                        text: new_text, suggestions: new_suggestions
                                    } => {
                                        text = new_text;
                                        command_suggestions = new_suggestions;
                                    },
                                    AutocompleteResult::Finished(command) => {
                                        match command {
                                            Command::TURN => {
                                                record_keeper.add_turn_update(mpv);
                                                text = String::new();
                                                command_suggestions = VecDeque::new();
                                                let pos = position().unwrap();
                                                let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                                println!("> Next turn started");
                                            },
                                            Command::QUIT => {
                                                break;
                                            },
                                            _ => {
                                            }
                                        }
                                    }
                                };
                            // Anything else
                            } else {
                                let autocomplete_result = get_user_input_for_autocomplete(&card_db.cards, &text, &card_suggestions, key);
                                match autocomplete_result {
                                    AutocompleteResult::Continue{
                                        text: new_text,
                                        suggestions: new_suggestions
                                    } => {
                                        text = new_text;
                                        card_suggestions = new_suggestions;
                                    },
                                    AutocompleteResult::Finished(card) => {
                                        if let Some(suggest) = card_suggestions.front() {
                                            let pos = position().unwrap();
                                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));
                                            println!("> {}", suggest.display);
                                            record_keeper.add_card_update(&mpv, &card.name, card.pitch);
                                            text = String::new();
                                            card_suggestions = VecDeque::new();
                                        }
                                    }
                                }
                            }
                            let pos = position().unwrap();
                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));

                            let display = {
                                if let Some(suggest) = card_suggestions.front() {
                                    let split = suggest.display.split_at(text.len());
                                    &format!("{}{}", split.0, split.1.grey())
                                } else if let Some(suggest) = command_suggestions.front()  {
                                    let split = suggest.get_name().split_at(text.len());
                                    &format!("{}{}", split.0, split.1.grey())
                                } else {
                                    &text
                                }
                            };
                            println!("> {}", display);
                            let _ = execute!(stdout(), MoveUp(1));
                        }
                    },
                    Some(Err(e)) => println!("Error: {:?}\r", e),
                    None => break,
                };
            }
        }
        if !text.is_empty() || !card_suggestions.is_empty() || !command_suggestions.is_empty() {
            if !mpv.get_property("pause").unwrap_or(true) {
                let _ = mpv.pause();
            }
        } else {
            if mpv.get_property("pause").unwrap_or(true) {
                let _ = mpv.unpause();
            }
        }
    }

    let _ = write!(&mut output_file, "{}", Record::headers());
    record_keeper.sort_records();
    for rec in record_keeper.records {
        let _ = write!(output_file, "{}", rec.text());
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Verify args
    if args.len() < 2 {
        println!("Video file name missing");
        exit(0)
    }
    if args.len() < 4 {
        println!("Player name arguments missing");
        exit(0)
    }

    // Verify video fp
    let video_fp = &args[1];
    if !std::fs::exists(video_fp)? {
        println!("File does not exist");
        return Ok(());
    }
    println!("{}", video_fp);

    // Load video file
    // TODO: Improve error handling
    let mpv = Mpv::new().unwrap();
    mpv.playlist_load_files(&[(&video_fp, FileState::AppendPlay, None)])
        .unwrap();
    mpv.pause().unwrap();

    // Get player names
    let player1 = args[2].to_string();
    let player2 = args[3].to_string();
    let output_fp = format!(
        "annotations/{}_v_{}_{}.tsv",
        player1,
        player2,
        chrono::Local::now()
    );
    let card_db = lib::card::CardDB::init();

    let heroes = card_db.heroes();

    enable_raw_mode()?;
    println!("Enter hero 1:");
    let hero1 = lib::commands::enter_card(&heroes).await;
    println!("Enter hero 2:");
    let hero2 = lib::commands::enter_card(&heroes).await;
    println!("Enter player going first:");
    let options = Vec::from([
        lib::autocomplete::AutocompleteOption::new("1".to_string()),
        lib::autocomplete::AutocompleteOption::new("2".to_string()),
    ]);
    let first = lib::commands::get_user_input(&options).await;
    println!("Press ENTER to start:");
    let mut reader = EventStream::new();
    loop {
        let mut event = reader.next().fuse();
        select! {
            maybe_event = event => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if key.code == KeyCode::Enter {
                            break;
                        }
                    },
                    Some(Err(e)) => println!("Error: {:?}\r", e),
                    _ => {}
                };
            }
        }
    }

    let pos = position().unwrap();
    let _ = execute!(stdout(), MoveTo(0, pos.1));
    print!("> ");

    execute!(&mut stdout())?;

    handle_events(&output_fp, &mpv, &card_db, hero1, hero2, first.text()).await;

    disable_raw_mode()
}
