use std::ops::{Add, Sub};

pub struct Coord(f64, f64);

impl Add for Coord {
    type Output = Coord;
    fn add(self, rhs: Self) -> Self::Output {
        Coord(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl Sub for Coord {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Coord(rhs.0 - self.0, rhs.1 - self.1)
    }
}

impl Coord {
    pub fn scale(&self, scalar: f64) -> Self {
        Coord(self.0 * scalar, self.1 * scalar)
    }

    pub fn from_i32_i32(x: i32, y: i32) -> Self {
        Coord(x as f64, y as f64)
    }

    pub fn to_i32_i32(&self) -> (i32, i32) {
        (self.0 as i32, self.1 as i32)
    }

    pub fn x(&self) -> f64 {
        self.0
    }

    pub fn y(&self) -> f64 {
        self.1
    }
}
