use opencv::{
    core::{Rect, Size, UMat, UMatTraitConst},
    imgproc::resize_def,
    Error,
};

use crate::image::copy_to;

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
pub fn center_offset_f64(inner: f64, outer: f64) -> f64 {
    (outer - inner) / 2.0
}

#[derive(Copy, Clone)]
pub struct RelativeRoi {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    left_horizontal_buffer: f64,
    right_horizontal_buffer: f64,
    top_vertical_buffer: f64,
    bottom_vertical_buffer: f64,
}

impl RelativeRoi {
    pub fn get_height(&self) -> f64 {
        self.height
    }

    pub fn get_width(&self) -> f64 {
        self.width
    }

    fn validate_inputs(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        left_horizontal_buffer: f64,
        right_horizontal_buffer: f64,
        top_vertical_buffer: f64,
        bottom_vertical_buffer: f64,
    ) -> Result<(), RelativeRoiError> {
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
        if left_horizontal_buffer + right_horizontal_buffer >= width {
            return Err(RelativeRoiError(
                "`horizontal_buffer` must be less than half of  `width`.".to_string(),
            ));
        }

        // vertical_buffer check
        if top_vertical_buffer + bottom_vertical_buffer >= height {
            return Err(RelativeRoiError(
                "`vertical_buffer` must be less than half of `height`.".to_string(),
            ));
        }

        Ok(())
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
    pub fn build_def(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        horizontal_buffer: Option<f64>,
        vertical_buffer: Option<f64>,
    ) -> Result<Self, RelativeRoiError> {
        let horizontal_buffer = horizontal_buffer.unwrap_or(0.0);
        let vertical_buffer = vertical_buffer.unwrap_or(0.0);
        Self::validate_inputs(
            x,
            y,
            width,
            height,
            horizontal_buffer,
            horizontal_buffer,
            vertical_buffer,
            vertical_buffer,
        )?;

        Ok(Self {
            x,
            y,
            width,
            height,
            left_horizontal_buffer: horizontal_buffer,
            right_horizontal_buffer: horizontal_buffer,
            top_vertical_buffer: vertical_buffer,
            bottom_vertical_buffer: vertical_buffer,
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
        let horizontal_buffer = horizontal_buffer.unwrap_or(0.0);
        let vertical_buffer = vertical_buffer.unwrap_or(0.0);

        let (left_horizontal_buffer, right_horizontal_buffer) = {
            if let Some(part) = horizontal_partition {
                part.split_horizontal_buffer(horizontal_buffer)
            } else {
                (horizontal_buffer, horizontal_buffer)
            }
        };

        let (top_vertical_buffer, bottom_vertical_buffer) = {
            if let Some(part) = vertical_partition {
                part.split_vertical_buffer(vertical_buffer)
            } else {
                (vertical_buffer, vertical_buffer)
            }
        };

        Self::validate_inputs(
            x,
            y,
            width,
            height,
            left_horizontal_buffer,
            right_horizontal_buffer,
            top_vertical_buffer,
            bottom_vertical_buffer,
        )?;

        Ok(Self {
            x,
            y,
            width,
            height,
            left_horizontal_buffer,
            right_horizontal_buffer,
            top_vertical_buffer,
            bottom_vertical_buffer,
        })
    }

    pub fn build(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        left_horizontal_buffer: f64,
        right_horizontal_buffer: f64,
        top_vertical_buffer: f64,
        bottom_vertical_buffer: f64,
    ) -> Result<Self, RelativeRoiError> {
        Self::validate_inputs(
            x,
            y,
            width,
            height,
            left_horizontal_buffer,
            right_horizontal_buffer,
            top_vertical_buffer,
            bottom_vertical_buffer,
        )?;

        Ok(Self {
            x,
            y,
            width,
            height,
            left_horizontal_buffer,
            right_horizontal_buffer,
            top_vertical_buffer,
            bottom_vertical_buffer,
        })
    }

    /// Generates rect given full frame size
    pub fn generate_roi(&self, region_size: &Size, umat: &UMat) -> Rect {
        // calculate ratio
        let ratio = umat.cols() as f64 / umat.rows() as f64;

        // calculate buffer dimensions
        let left_horizontal_buffer = self.left_horizontal_buffer * region_size.width as f64;
        let right_horizontal_buffer = self.right_horizontal_buffer * region_size.width as f64;
        let top_vertical_buffer = self.top_vertical_buffer * region_size.height as f64;
        let bottom_vertical_buffer = self.bottom_vertical_buffer * region_size.height as f64;

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
        let left_horizontal_buffer = self.left_horizontal_buffer * region_size.width as f64;
        let right_horizontal_buffer = self.right_horizontal_buffer * region_size.width as f64;
        let top_vertical_buffer = self.top_vertical_buffer * region_size.height as f64;
        let bottom_vertical_buffer = self.bottom_vertical_buffer * region_size.height as f64;

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

    pub fn copy_to(&self, img: &UMat, frame: &mut UMat) -> Result<(), Box<dyn std::error::Error>> {
        let roi_rect = self.generate_roi(&frame.size()?, img);
        let resized = self.resize(&frame.size()?, img)?;
        copy_to(&resized, frame, &roi_rect)
    }

    fn scale_rel(&self, scale: f64) -> Result<Self, Box<dyn std::error::Error>> {
        let new_width = self.width * scale;
        let new_height = self.height * scale;
        let x_offset = center_offset_f64(self.width, new_width);
        let y_offset = center_offset_f64(self.height, new_height);

        let new_x = self.x - x_offset;
        let new_y = self.y - y_offset;

        let new_rel = Self::build(
            new_x,
            new_y,
            new_width,
            new_height,
            self.left_horizontal_buffer,
            self.right_horizontal_buffer,
            self.top_vertical_buffer,
            self.bottom_vertical_buffer,
        )?;
        Ok(new_rel)
    }

    pub fn scale_rel_safe(&self, scale: f64) -> Result<Self, Box<dyn std::error::Error>> {
        if scale <= 0.0 {
            return Err(RelativeRoiError(format!(
                "Cannot scale a RelativeRoi by a non-positive number: {}",
                scale
            ))
            .into());
        }

        if scale <= 1.0 {
            return self.scale_rel(scale);
        }

        let left_bound = (self.width + 2.0 * self.x) / self.width;
        let right_bound = (self.width + 2.0 * (1.0 - self.x - self.width)) / self.width;
        let top_bound = (self.height + 2.0 * self.y) / self.height;
        let bottom_bound = (self.height + 2.0 * (1.0 - self.y - self.height)) / self.height;

        let minimum = [left_bound, right_bound, top_bound, bottom_bound]
            .into_iter()
            .reduce(f64::min)
            .unwrap();

        self.scale_rel(minimum)
    }
}

#[derive(Copy, Clone)]
pub enum HorizontalPartition {
    Left,
    Center,
    Right,
}

impl HorizontalPartition {
    fn split_horizontal_buffer(&self, buffer: f64) -> (f64, f64) {
        match self {
            HorizontalPartition::Left => (buffer, 0.5 * buffer),
            HorizontalPartition::Right => (0.5 * buffer, buffer),
            HorizontalPartition::Center => (buffer, buffer),
        }
    }
}

#[derive(Copy, Clone)]
pub enum VerticalPartition {
    Top,
    Center,
    Bottom,
}

impl VerticalPartition {
    fn split_vertical_buffer(&self, buffer: f64) -> (f64, f64) {
        match self {
            VerticalPartition::Top => (buffer, 0.5 * buffer),
            VerticalPartition::Center => (buffer, buffer),
            VerticalPartition::Bottom => (0.5 * buffer, buffer),
        }
    }
}
