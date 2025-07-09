use opencv::{
    core::{Rect, Size, UMat, UMatTraitConst},
    imgproc::resize_def,
    Error,
};

#[derive(Debug)]
pub struct RelativeRoiError(String);

impl std::fmt::Display for RelativeRoiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RelativeRoiError::{}", self.0)
    }
}

impl std::error::Error for RelativeRoiError {}

pub fn center_offset(inner: i32, outer: i32) -> i32 {
    (outer - inner).div_euclid(2)
}

#[derive(Copy, Clone)]
pub struct RelativeRoi {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    horizontal_buffer: f64,
    vertical_buffer: f64,
    horizontal_partition: Option<HorizontalPartition>,
    vertical_partition: Option<VerticalPartition>,
}

impl RelativeRoi {
    /// # Arguments
    /// * `x` - x-axis offset proportional to the whole frame
    /// * `y` - y-axis offset proportional to the whole frame
    /// * `width` - width of the subregion proportional to the whole frame
    /// * `height` - height of the subregion proportional to the whole frame
    /// * `horizontal_buffer` - optional value to set a buffer between the left and right sides of
    /// the subregion
    /// * `vertical_buffer` - optional value to set a buffer between the top and bottom of the sub
    /// region
    pub fn build(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        horizontal_buffer: Option<f64>,
        vertical_buffer: Option<f64>,
    ) -> Result<Self, RelativeRoiError> {
        // `x` is a valid percentage
        if x < 0.0 || x > 1.0 {
            return Err(RelativeRoiError(
                "`x` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // `y` is a valid percentage
        if y < 0.0 || y > 1.0 {
            return Err(RelativeRoiError(
                "`y` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // `width` is a valid percentage
        if width < 0.0 || width > 1.0 {
            return Err(RelativeRoiError(
                "`width` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // `height` is a valid percentage
        if height < 0.0 || height > 1.0 {
            return Err(RelativeRoiError(
                "`height` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // x dimensions add up to less than 1
        if width + x > 1.0 {
            return Err(RelativeRoiError(
                "`x` and `width` cannot add up to more than 1.".to_string(),
            ));
        }

        // y dimensions add up to less than 1
        if height + y > 1.0 {
            return Err(RelativeRoiError(
                "`y` and `height` cannot add up to more than 1.".to_string(),
            ));
        }

        // horizontal_buffer check
        if 2.0 * horizontal_buffer.unwrap_or(0.0) >= width {
            return Err(RelativeRoiError(
                "`horizontal_buffer` must be less than half of  `width`.".to_string(),
            ));
        }

        // vertical_buffer check
        if 2.0 * vertical_buffer.unwrap_or(0.0) >= height {
            return Err(RelativeRoiError(
                "`vertical_buffer` must be less than half of `height`.".to_string(),
            ));
        }

        Ok(Self {
            x,
            y,
            width,
            height,
            horizontal_buffer: horizontal_buffer.unwrap_or(0.0),
            vertical_buffer: vertical_buffer.unwrap_or(0.0),
            horizontal_partition: None,
            vertical_partition: None,
        })
    }

    /// # Arguments
    /// * `x` - x-axis offset proportional to the whole frame
    /// * `y` - y-axis offset proportional to the whole frame
    /// * `width` - width of the subregion proportional to the whole frame
    /// * `height` - height of the subregion proportional to the whole frame
    /// * `horizontal_buffer` - optional value to set a buffer between the left and right sides of
    /// the subregion
    /// * `vertical_buffer` - optional value to set a buffer between the top and bottom of the sub
    /// region
    pub fn build_as_partition(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        horizontal_buffer: Option<f64>,
        vertical_buffer: Option<f64>,
        horizontal_partition: Option<HorizontalPartition>,
        vertical_partition: Option<VerticalPartition>,
    ) -> Result<Self, RelativeRoiError> {
        // `x` is a valid percentage
        if x < 0.0 || x > 1.0 {
            return Err(RelativeRoiError(
                "`x` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // `y` is a valid percentage
        if y < 0.0 || y > 1.0 {
            return Err(RelativeRoiError(
                "`y` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // `width` is a valid percentage
        if width < 0.0 || width > 1.0 {
            return Err(RelativeRoiError(
                "`width` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // `height` is a valid percentage
        if height < 0.0 || height > 1.0 {
            return Err(RelativeRoiError(
                "`height` value cannot be less than 0 or greater than 1.".to_string(),
            ));
        }

        // x dimensions add up to less than 1
        if width + x > 1.0 {
            return Err(RelativeRoiError(
                "`x` and `width` cannot add up to more than 1.".to_string(),
            ));
        }

        // y dimensions add up to less than 1
        if height + y > 1.0 {
            return Err(RelativeRoiError(
                "`y` and `height` cannot add up to more than 1.".to_string(),
            ));
        }

        // horizontal_buffer check
        if 2.0 * horizontal_buffer.unwrap_or(0.0) >= width {
            return Err(RelativeRoiError(
                "`horizontal_buffer` must be less than half of  `width`.".to_string(),
            ));
        }

        // vertical_buffer check
        if 2.0 * vertical_buffer.unwrap_or(0.0) >= height {
            return Err(RelativeRoiError(
                "`vertical_buffer` must be less than half of `height`.".to_string(),
            ));
        }

        Ok(Self {
            x,
            y,
            width,
            height,
            horizontal_buffer: horizontal_buffer.unwrap_or(0.0),
            vertical_buffer: vertical_buffer.unwrap_or(0.0),
            horizontal_partition,
            vertical_partition,
        })
    }

    /// Generates rect given full frame size
    pub fn generate_roi(&self, region_size: &Size, umat: &UMat) -> Rect {
        // calculate ratio
        let ratio = umat.cols() as f64 / umat.rows() as f64;

        // calculate buffer dimensions
        let horizontal_buffer = self.horizontal_buffer * region_size.width as f64;
        let vertical_buffer = self.vertical_buffer * region_size.height as f64;

        let left_horizontal_buffer = match self.horizontal_partition {
            None => horizontal_buffer,
            Some(HorizontalPartition::Left) => horizontal_buffer,
            Some(HorizontalPartition::Center) => horizontal_buffer * 0.5,
            Some(HorizontalPartition::Right) => horizontal_buffer * 0.5,
        };
        let right_horizontal_buffer = match self.horizontal_partition {
            None => horizontal_buffer,
            Some(HorizontalPartition::Left) => horizontal_buffer * 0.5,
            Some(HorizontalPartition::Center) => horizontal_buffer * 0.5,
            Some(HorizontalPartition::Right) => horizontal_buffer,
        };
        let top_vertical_buffer = match self.vertical_partition {
            None => vertical_buffer,
            Some(VerticalPartition::Top) => vertical_buffer,
            Some(VerticalPartition::Center) => vertical_buffer * 0.5,
            Some(VerticalPartition::Bottom) => vertical_buffer * 0.5,
        };
        let bottom_vertical_buffer = match self.vertical_partition {
            None => vertical_buffer,
            Some(VerticalPartition::Top) => vertical_buffer * 0.5,
            Some(VerticalPartition::Center) => vertical_buffer * 0.5,
            Some(VerticalPartition::Bottom) => vertical_buffer,
        };

        // calculate outer dimensions of subregion
        let outer_x = self.x * region_size.width as f64;
        let outer_y = self.y * region_size.height as f64;
        let outer_width = self.width * region_size.width as f64;
        let outer_height = self.height * region_size.height as f64;

        // adjust for buffer
        let outer_x = outer_x + left_horizontal_buffer;
        let outer_y = outer_y + top_vertical_buffer;
        let outer_width = outer_width - (left_horizontal_buffer + right_horizontal_buffer);
        let outer_height = outer_height - (top_vertical_buffer + bottom_vertical_buffer);

        // calculate potential dimensions based on ratio
        let potential_height = outer_width * ratio.recip();
        let potential_width = outer_height * ratio;

        // determine actual dimensions based on all restrictions
        let (width, height) = {
            if potential_width > outer_width {
                (outer_width, potential_height)
            } else {
                (potential_width, outer_height)
            }
        };

        // Convert to i32
        let width = width as i32;
        let height = height as i32;
        let outer_x = outer_x as i32;
        let outer_y = outer_y as i32;
        let outer_width = outer_width as i32;
        let outer_height = outer_height as i32;

        // calculate offset needed to center image
        let centered_width_offset = center_offset(width, outer_width);
        let centered_height_offset = center_offset(height, outer_height);

        Rect::new(
            outer_x + centered_width_offset,
            outer_y + centered_height_offset,
            width as i32,
            height as i32,
        )
    }

    /// Generates rect given full frame size
    pub fn generate_roi_raw(&self, region_size: &Size) -> Rect {
        // calculate buffer dimensions
        let horizontal_buffer = self.horizontal_buffer * region_size.width as f64;
        let vertical_buffer = self.vertical_buffer * region_size.height as f64;

        let left_horizontal_buffer = match self.horizontal_partition {
            None => horizontal_buffer,
            Some(HorizontalPartition::Left) => horizontal_buffer,
            Some(HorizontalPartition::Center) => horizontal_buffer * 0.5,
            Some(HorizontalPartition::Right) => horizontal_buffer * 0.5,
        };
        let right_horizontal_buffer = match self.horizontal_partition {
            None => horizontal_buffer,
            Some(HorizontalPartition::Left) => horizontal_buffer * 0.5,
            Some(HorizontalPartition::Center) => horizontal_buffer * 0.5,
            Some(HorizontalPartition::Right) => horizontal_buffer,
        };
        let top_vertical_buffer = match self.vertical_partition {
            None => vertical_buffer,
            Some(VerticalPartition::Top) => vertical_buffer,
            Some(VerticalPartition::Center) => vertical_buffer * 0.5,
            Some(VerticalPartition::Bottom) => vertical_buffer * 0.5,
        };
        let bottom_vertical_buffer = match self.vertical_partition {
            None => vertical_buffer,
            Some(VerticalPartition::Top) => vertical_buffer * 0.5,
            Some(VerticalPartition::Center) => vertical_buffer * 0.5,
            Some(VerticalPartition::Bottom) => vertical_buffer,
        };

        // calculate outer dimensions of subregion
        let outer_x = self.x * region_size.width as f64;
        let outer_y = self.y * region_size.height as f64;
        let outer_width = self.width * region_size.width as f64;
        let outer_height = self.height * region_size.height as f64;

        // adjust for buffer
        let outer_x = outer_x + left_horizontal_buffer;
        let outer_y = outer_y + top_vertical_buffer;
        let outer_width = outer_width - (left_horizontal_buffer + right_horizontal_buffer);
        let outer_height = outer_height - (top_vertical_buffer + bottom_vertical_buffer);

        // Convert to i32
        let outer_x = outer_x as i32;
        let outer_y = outer_y as i32;
        let outer_width = outer_width as i32;
        let outer_height = outer_height as i32;

        Rect::new(outer_x, outer_y, outer_width, outer_height)
    }

    pub fn resize(&self, region_size: &Size, umat: &UMat) -> Result<UMat, Error> {
        let rect = self.generate_roi(region_size, umat);
        let mut output = UMat::new_def();
        resize_def(umat, &mut output, rect.size())?;
        Ok(output)
    }
}

#[derive(Copy, Clone)]
pub enum HorizontalPartition {
    Left,
    Center,
    Right,
}

#[derive(Copy, Clone)]
pub enum VerticalPartition {
    Top,
    Center,
    Bottom,
}
