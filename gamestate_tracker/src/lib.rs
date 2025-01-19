mod autocomplete;
mod card_db;
mod display;

use std::fmt::Display;

use display::merge_displays;

pub enum Color {
    Red,
    Yellow,
    Blue,
    Colorless,
}

pub struct Card {
    name: String,
    color: Color,
}

pub struct Deck(Vec<Card>);

pub struct ActiveLink {
    threatening: u32,
    defending: u32,
    preventing: u32,
}

pub struct ChainLink {
    hit: bool,
    dealt: u32,
}

pub enum PitchGroup {
    Unknown(usize),
    Group(Vec<Card>),
}

impl PitchGroup {
    fn build<I: Iterator<Item = Card>>(cards: I) -> Self {
        PitchGroup::Group(cards.collect())
    }
}

pub struct PlayerAnalytics {
    hero: String,
    name: String,
    health: u32,
    cards_in_deck: usize,
    cards_in_hand: usize,
    pitch: Vec<Card>,
    pitch_stack: Vec<PitchGroup>,
    intellect: usize,
}

impl PlayerAnalytics {
    fn get_health(hero: &str) -> u32 {
        match hero {
            "riptide" => 19,
            "riptide, lurker of the deep" => 38,
            "dash io" => 36,
            "dash database" => 18,
            _ => 40,
        }
    }

    fn get_intellect(hero: &str) -> usize {
        if hero.trim().to_lowercase() == "datadoll" {
            3
        } else {
            4
        }
    }

    fn build(player: &Player) -> Self {
        let health = Self::get_health(&player.hero);
        let intellect = Self::get_intellect(&player.hero);
        let cards_in_deck = player.deck_size - intellect;

        PlayerAnalytics {
            hero: player.hero.clone(),
            name: player.name.clone(),
            health,
            cards_in_deck,
            cards_in_hand: intellect,
            pitch: Vec::new(),
            pitch_stack: vec![PitchGroup::Unknown(cards_in_deck)],
            intellect,
        }
    }

    fn draw(&mut self, num_cards: usize) {
        self.cards_in_deck -= 1;
    }

    fn to_display(&self) -> Vec<String> {
        let mut display = Vec::new();
        // Name
        display.push(format!("Player: {}", &self.name));
        // Hero
        display.push(format!("Hero: {}", &self.hero));
        // Health
        display.push(format!("Health: {}", &self.health));
        // Cards in Hand
        display.push(format!("Cards in Hand: {}", self.cards_in_hand));
        // Cards in Deck
        display.push(format!(
            "Cards in Deck: {}",
            self.cards_in_deck + self.pitch.len()
        ));
        display
    }
}

impl std::fmt::Display for PlayerAnalytics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut display = String::new();
        // Name
        display.push_str(&format!("Player: {}\n", &self.name));
        // Hero
        display.push_str(&format!("Hero: {}\n", &self.hero));
        // Health
        display.push_str(&format!("Health: {}\n", &self.health));
        // Cards in Hand
        display.push_str(&format!("Cards in Hand: {}\n", self.cards_in_hand));
        // Cards in Deck
        display.push_str(&format!(
            "Cards in Deck: {}\n",
            self.cards_in_deck + self.pitch.len()
        ));
        write!(f, "{}", display)
    }
}

pub struct GameState {
    turn_number: u32,
    turn_player_1: bool,
    chain: Vec<ChainLink>,
    active_link: Option<ActiveLink>,
    player_1: PlayerAnalytics,
    player_2: PlayerAnalytics,
    action_points: u32,
    resources: u32,
}

impl Display for GameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut display = String::new();
        let turn_player = if self.turn_player_1 {
            &self.player_1.name
        } else {
            &self.player_2.name
        };

        display.push_str(&format!("Turn: {}\n", self.turn_number));
        display.push_str(&format!("Turn Player: {}\n", turn_player));
        display.push_str("\n");
        let player_1_display = self.player_1.to_display();
        let player_2_display = self.player_2.to_display();
        let merged_displays = merge_displays(player_1_display, player_2_display);
        display.push_str(&merged_displays);

        write!(f, "{}", display)
    }
}

pub struct Player {
    pub name: String,
    pub hero: String,
    pub deck_size: usize,
}

impl GameState {
    pub fn build_cc(player_1: Player, player_2: Player, turn_player_1: bool) -> Self {
        GameState {
            turn_number: 0,
            turn_player_1,
            chain: Vec::new(),
            active_link: None,
            player_1: PlayerAnalytics::build(&player_1),
            player_2: PlayerAnalytics::build(&player_2),
            action_points: 1,
            resources: 0,
        }
    }

    fn turn_player(&mut self) -> &mut PlayerAnalytics {
        if self.turn_player_1 {
            &mut self.player_1
        } else {
            &mut self.player_2
        }
    }

    pub fn play_from_hand(&mut self, _card_name: String, _color: Color) {
        let player = self.turn_player();
        player.cards_in_hand -= 1;
    }

    pub fn pitch(&mut self, cards: Vec<Card>) {
        let player = self.turn_player();
        player.cards_in_hand -= cards.len();
        player.pitch.extend(cards);
    }

    pub fn end_turn(&mut self) {
        let player = self.turn_player();
        // Put pitch on bottom
        let pitch = player.pitch.drain(..);
        player.pitch_stack.push(PitchGroup::build(pitch));
        // Draw up

        // Other player draws up if turn number is 0

        // Change turn player
    }
}
