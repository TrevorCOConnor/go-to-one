use chrono;
use csv::StringRecord;
use std::{
    collections::VecDeque,
    fs::File,
    io::{stdin, stdout, Write},
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
    autocomplete::{get_user_input_for_autocomplete, AutocompleteResult, Named},
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

enum Command {
    PAUSE,
    HEALTH,
    TURN,
    QUIT,
}

impl Command {
    fn get_all() -> Vec<Self> {
        Vec::from([
            Command::HEALTH,
            Command::PAUSE,
            Command::TURN,
            Command::QUIT,
        ])
    }
}

impl Named for Command {
    fn get_name(&self) -> &str {
        match self {
            Command::PAUSE => ":p",
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

fn extract_life_update(text: &str) -> Option<(u8, char, u32)> {
    let mut player = None;
    let mut operation = None;
    let mut value = None;

    let splits: Vec<&str> = text.split(" ").filter(|v| !v.is_empty()).collect();
    if splits.len() == 4 && splits.first() == Some(&&":h") {
        if splits[1] == "1" {
            player = Some(1);
        }
        if splits[1] == "2" {
            player = Some(2);
        }
        if ["+", "-", "="].contains(&splits[2]) {
            operation = Some(splits[2].chars().next().unwrap());
        }
        if let Ok(val) = splits[3].parse::<u32>() {
            value = Some(val);
        }
    }
    if player.is_some() && operation.is_some() && value.is_some() {
        return Some((player.unwrap(), operation.unwrap(), value.unwrap()));
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
    player1_life: Option<u32>,
    player2_life: Option<u32>,
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
            self.player1_life.map_or("".to_string(), |v| v.to_string()),
            self.player2_life.map_or("".to_string(), |v| v.to_string()),
            self.update_type.text()
        )
    }
}

struct RecordKeeper {
    start_time: time::Instant,
    player1_life: u32,
    player2_life: u32,
    records: Vec<Record>,
    paused: bool,
    pause_time: Option<time::Instant>,
    pause_offset: Duration,
}

impl RecordKeeper {
    fn build(hero1: &CardData, hero2: &CardData, first: &str) -> RecordKeeper {
        let mut rk = RecordKeeper {
            start_time: time::Instant::now(),
            player1_life: hero1.life.expect("Hero1 should have life"),
            player2_life: hero2.life.expect("Hero2 should have life"),
            records: Vec::new(),
            paused: false,
            pause_time: None,
            pause_offset: Duration::from_secs(0),
        };

        let hero1_record = Record {
            sec: 0,
            milli: 0,
            name: Some(hero1.name.to_owned()),
            pitch: None,
            player1_life: Some(hero1.life.unwrap()),
            player2_life: None,
            update_type: UpdateType::Hero1,
        };
        let hero2_record = Record {
            sec: 0,
            milli: 0,
            name: Some(hero2.name.to_owned()),
            pitch: None,
            player1_life: None,
            player2_life: Some(hero2.life.unwrap()),
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

    fn add_card_update(&mut self, name: &str, pitch: Option<u32>) {
        let diff = time::Instant::now() - self.start_time;
        let milli = diff.as_millis().rem_euclid(MILLI as u128);
        self.records.push(Record {
            sec: diff.as_secs(),
            milli,
            name: Some(name.to_owned()),
            pitch,
            player1_life: None,
            player2_life: None,
            update_type: UpdateType::Card,
        });
    }

    fn get_time(&self) -> (u64, u128) {
        let diff = time::Instant::now() - self.start_time - self.pause_offset;
        let secs = diff.as_secs();
        let milli = diff.as_millis().rem_euclid(MILLI as u128);
        (secs, milli)
    }

    fn pause(&mut self) {
        if self.paused {
            self.paused = false;
            self.pause_offset += time::Instant::now() - self.pause_time.unwrap();
            self.pause_time = None;
        } else {
            self.paused = true;
            self.pause_time = Some(time::Instant::now());
        }
    }

    fn add_player_life_update(&mut self, player: u8, operation: &char, value: u32) {
        let (sec, milli) = self.get_time();
        // Get old life of player
        let old_life = if player == 1 {
            self.player1_life
        } else {
            self.player2_life
        };

        // Calculate new life
        let new_value = {
            match operation {
                '=' => value,
                '+' => old_life + value,
                '-' => old_life.checked_sub(value).unwrap_or(0),
                _ => panic!("Invalid operation given"),
            }
        };

        // Update life tracker
        if player == 1 {
            self.player1_life = new_value;
        } else {
            self.player2_life = new_value;
        }

        // Save record
        let player1_new_life = if player == 1 { Some(new_value) } else { None };
        let player2_new_life = if player == 2 { Some(new_value) } else { None };
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

    fn add_turn_update(&mut self) {
        let (sec, milli) = self.get_time();
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
}

async fn handle_events(
    output_fp: &str,
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

    loop {
        let mut event = reader.next().fuse();
        select! {
            maybe_event = event => {
                match maybe_event {
                    Some(Ok(event)) => {
                        if let Event::Key(key) = event {
                            if record_keeper.paused {
                                match key.code {
                                    KeyCode::Enter => {},
                                    KeyCode::Char(' ') => {}
                                    KeyCode::Esc => {}
                                    _ => {
                                        continue;
                                    }
                                }
                                record_keeper.pause();
                            }
                            else if is_life_update(&text) {
                                command_suggestions = VecDeque::new();
                                match key.code {
                                    KeyCode::Enter => {
                                        if let Some((player, operation, value)) = extract_life_update(&text) {
                                            record_keeper.add_player_life_update(player, &operation, value);
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
                                    },
                                    _ => continue
                                }
                            }
                            else if is_command(&text) || (text.is_empty() && key.code == KeyCode::Char(':')) {
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
                                            Command::PAUSE => {
                                                record_keeper.pause();
                                                text = String::new();
                                                command_suggestions = VecDeque::new();
                                            },
                                            Command::TURN => {
                                                record_keeper.add_turn_update();
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
                                            record_keeper.add_card_update(&card.name, card.pitch);
                                            text = String::new();
                                            card_suggestions = VecDeque::new();
                                        }
                                    }
                                }
                            }
                            let pos = position().unwrap();
                            let _  = execute!(stdout(), MoveTo(0, pos.1), Clear(ClearType::CurrentLine));

                            let display = {
                                if record_keeper.paused {
                                    "PAUSED"
                                } else {
                                    if let Some(suggest) = card_suggestions.front() {
                                        let split = suggest.display.split_at(text.len());
                                        &format!("{}{}", split.0, split.1.grey())
                                    } else if let Some(suggest) = command_suggestions.front()  {
                                        let split = suggest.get_name().split_at(text.len());
                                        &format!("{}{}", split.0, split.1.grey())
                                    } else {
                                        &text
                                    }
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
    }

    let _ = write!(&mut output_file, "{}", Record::headers());
    for rec in record_keeper.records {
        let _ = write!(output_file, "{}", rec.text());
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

    println!("Press ENTER to start the timer:");
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
    println!("Timer started!");
    let pos = position().unwrap();
    let _ = execute!(stdout(), MoveTo(0, pos.1));
    print!("> ");

    let mut stdout = stdout();
    execute!(stdout)?;

    handle_events(&output_fp, &card_db, hero1, hero2, first.text()).await;

    disable_raw_mode()
}
