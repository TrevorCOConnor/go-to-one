use core::panic;

pub enum Operation {
    Add,
    Sub,
    Equal,
}

impl Operation {
    fn from_char(c: &char) -> Option<Self> {
        match c {
            '+' => Some(Self::Add),
            '-' => Some(Self::Sub),
            '=' => Some(Self::Equal),
            _ => None,
        }
    }
}

pub struct LifeTracker {
    current: i32,
    display: i32,
    ticker: u32,
    ticker_max: u32,
}

impl LifeTracker {
    /// # Arguments
    /// * `starting_life` - String rep of the heros starting life
    /// * `tick_rate` - How often the tracker should be updated
    /// * `increment` - How much time elapses each frame
    pub fn build(starting_life: &str, tick_rate: f64, increment: f64) -> Self {
        let value = starting_life
            .parse::<i32>()
            .expect("Starting life is not a number");
        let ticker_max = (tick_rate / increment) as u32;
        LifeTracker {
            current: value,
            display: value,
            ticker: 0,
            ticker_max,
        }
    }

    pub fn parse_update(update: &str) -> Result<(Operation, i32), String> {
        let operation_char = update.chars().next().ok_or("Update missing operation")?;
        let operation =
            Operation::from_char(&operation_char).ok_or("Update does not have valid operation")?;

        let val = update
            .get(1..)
            .expect("Update missing value")
            .parse::<u32>()
            .map_err(|_| "Update value is not an integer")?;

        Ok((operation, val as i32))
    }

    pub fn update(&mut self, update: &str) {
        let update = Self::parse_update(update);
        if let Err(err) = update {
            panic!("{}", err);
        }
        let (operation, val) = update.unwrap();
        let new_value = {
            match operation {
                Operation::Add => self.current + val,
                Operation::Sub => self.current - val,
                Operation::Equal => val,
            }
        };
        self.current = new_value;
    }

    /// Ticks display life by one increment
    pub fn tick_display(&mut self) {
        self.ticker += 1;
        if self.ticker == self.ticker_max {
            self.ticker = 0;
            self.display += (self.current - self.display).signum();
        }
    }

    pub fn display(&self) -> String {
        self.display.to_string()
    }
}
