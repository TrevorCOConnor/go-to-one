use std::error;

#[derive(Debug, Clone)]
pub enum RoiError {
    TooWide,
    TooTall,
    NegativeScale,
}

impl error::Error for RoiError {}

impl std::fmt::Display for RoiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoiError::TooWide => {
                write!(f, "Region is too wide for the frame")
            }
            RoiError::TooTall => {
                write!(f, "Region is too tall for the frame")
            }
            RoiError::NegativeScale => {
                write!(f, "Cannot scale a region by a negative number")
            }
        }
    }
}
