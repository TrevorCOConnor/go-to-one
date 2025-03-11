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
}

impl LifeTracker {
    pub fn build(starting_life: &str) -> Self {
        let value = starting_life
            .parse::<i32>()
            .expect("Starting life is not a number");
        LifeTracker {
            current: value,
            display: value,
        }
    }

    pub fn parse_update(update: &str) -> Result<(Operation, i32), String> {
        let operation_char = update.chars().next().ok_or("Update missing operation")?;
        let operation =
            Operation::from_char(&operation_char).ok_or("Update does not have valid operation")?;

        let val = update
            .get(1..)
            .expect("Update missing value")
            .parse::<i32>()
            .map_err(|_| "Update value is not an integer")?;

        Ok((operation, val))
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

    pub fn tick_display(&mut self) {
        self.display += (self.current - self.display).signum();
    }

    pub fn display(&self) -> String {
        self.display.to_string()
    }
}
