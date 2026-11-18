// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! 1-bit framebuffer and drawing primitives.
//!
//! The default framebuffer is 800x480. Pixels are addressed in **logical** coordinates after
//! rotation; storage stays in physical panel order. `1` means black and `0` means white.
//! Drawing never panics on an out-of-range coordinate: the pixel is clipped and the call reports
//! whether it changed anything.

use crate::profile::{
    DEFAULT_FRAME_BYTES, DEFAULT_HEIGHT, DEFAULT_ROW_BYTES, DEFAULT_WIDTH, PackError, PanelProfile,
    Waveshare75V2,
};

/// Display rotation in 90-degree clockwise steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    /// No rotation.
    #[default]
    None,
    /// 90 degrees clockwise.
    Quarter,
    /// 180 degrees.
    Half,
    /// 270 degrees clockwise (90 degrees counter-clockwise).
    ThreeQuarter,
}

impl Rotation {
    /// Logical `(width, height)` for a physical panel of `width` x `height`.
    #[must_use]
    pub const fn dimensions(self, width: usize, height: usize) -> (usize, usize) {
        match self {
            Self::None | Self::Half => (width, height),
            Self::Quarter | Self::ThreeQuarter => (height, width),
        }
    }
}

/// A 1-bit, MSB-first glyph or bitmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph<'a> {
    /// Glyph width in pixels.
    pub width: usize,
    /// Glyph height in pixels.
    pub height: usize,
    /// Row-major, MSB-first bits; each row occupies `(width + 7) / 8` bytes.
    pub data: &'a [u8],
}

/// A fixed 800x480 1-bit framebuffer with logical rotation.
pub struct Framebuffer {
    data: [u8; DEFAULT_FRAME_BYTES],
    rotation: Rotation,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    /// Create a white 800x480 framebuffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            data: [0; DEFAULT_FRAME_BYTES],
            rotation: Rotation::None,
        }
    }

    /// Physical panel width.
    #[must_use]
    pub const fn panel_width(&self) -> usize {
        DEFAULT_WIDTH
    }

    /// Physical panel height.
    #[must_use]
    pub const fn panel_height(&self) -> usize {
        DEFAULT_HEIGHT
    }

    /// Current rotation.
    #[must_use]
    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// Change the rotation; the stored physical pixels are not moved.
    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
    }

    /// Logical width after rotation.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.rotation
            .dimensions(self.panel_width(), self.panel_height())
            .0
    }

    /// Logical height after rotation.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.rotation
            .dimensions(self.panel_width(), self.panel_height())
            .1
    }

    /// Set every physical pixel to black (`true`) or white (`false`).
    pub fn clear(&mut self, black: bool) {
        self.data.fill(if black { 0xff } else { 0x00 });
    }

    /// Read a logical pixel; false for an out-of-range coordinate.
    #[must_use]
    pub fn pixel(&self, x: i32, y: i32) -> bool {
        let Some((px, py)) = self.map(x, y) else {
            return false;
        };
        self.physical_pixel(px, py)
    }

    /// Read a physical pixel; false when out of the panel.
    #[must_use]
    pub fn physical_pixel(&self, x: usize, y: usize) -> bool {
        if x >= self.panel_width() || y >= self.panel_height() {
            return false;
        }
        let Some(byte) = self.data.get(y.saturating_mul(DEFAULT_ROW_BYTES) + x / 8) else {
            return false;
        };
        byte & (0x80 >> (x % 8)) != 0
    }

    /// Set a logical pixel; returns true when it was inside the panel.
    pub fn set_pixel(&mut self, x: i32, y: i32, black: bool) -> bool {
        let Some((px, py)) = self.map(x, y) else {
            return false;
        };
        let index = py.saturating_mul(DEFAULT_ROW_BYTES) + px / 8;
        let Some(byte) = self.data.get_mut(index) else {
            return false;
        };
        let mask = 0x80 >> (px % 8);
        if black {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
        true
    }

    /// Draw a line, clipping every pixel at the panel edge.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, black: bool) {
        let mut x = i64::from(x0);
        let mut y = i64::from(y0);
        let end_x = i64::from(x1);
        let end_y = i64::from(y1);
        let dx = (end_x - x).abs();
        let sx = if x < end_x { 1 } else { -1 };
        let dy = -(end_y - y).abs();
        let sy = if y < end_y { 1 } else { -1 };
        let mut error = dx.saturating_add(dy);
        loop {
            let _ = self.set_pixel(clamp_i32(x), clamp_i32(y), black);
            if x == end_x && y == end_y {
                break;
            }
            let twice = error.saturating_mul(2);
            if twice >= dy {
                error = error.saturating_add(dy);
                x = x.saturating_add(sx);
            }
            if twice <= dx {
                error = error.saturating_add(dx);
                y = y.saturating_add(sy);
            }
        }
    }

    /// Fill a rectangle, clipping at the panel edge.
    pub fn rect(&mut self, x: i32, y: i32, width: usize, height: usize, black: bool) {
        let height = i32::try_from(height).unwrap_or(i32::MAX);
        let width = i32::try_from(width).unwrap_or(i32::MAX);
        let mut row = 0i32;
        while row < height {
            let mut column = 0i32;
            while column < width {
                let _ = self.set_pixel(x.saturating_add(column), y.saturating_add(row), black);
                column = column.saturating_add(1);
            }
            row = row.saturating_add(1);
        }
    }

    /// Blit an MSB-first 1-bit bitmap; a set bit paints `black`. Returns painted pixel count.
    pub fn blit_bitmap(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        data: &[u8],
        black: bool,
    ) -> usize {
        let row_bytes = width.saturating_add(7) / 8;
        let mut painted = 0usize;
        let mut row = 0usize;
        while row < height {
            let Some(start) = row.checked_mul(row_bytes) else {
                break;
            };
            let mut column = 0usize;
            while column < width {
                let Some(byte) = data.get(start.saturating_add(column / 8)).copied() else {
                    break;
                };
                if byte & (0x80 >> (column % 8)) != 0 {
                    let px = x.saturating_add(i32::try_from(column).unwrap_or(i32::MAX));
                    let py = y.saturating_add(i32::try_from(row).unwrap_or(i32::MAX));
                    if self.set_pixel(px, py, black) {
                        painted = painted.saturating_add(1);
                    }
                }
                column = column.saturating_add(1);
            }
            row = row.saturating_add(1);
        }
        painted
    }

    /// Blit a [`Glyph`]; a set bit paints `black`. Returns painted pixel count.
    pub fn blit_glyph(&mut self, x: i32, y: i32, glyph: &Glyph<'_>, black: bool) -> usize {
        self.blit_bitmap(x, y, glyph.width, glyph.height, glyph.data, black)
    }

    /// Pack the physical frame for `P`.
    ///
    /// The output length must be at least `P::ROW_BYTES * P::HEIGHT`. The return value is the
    /// number of bytes written.
    pub fn pack<P: PanelProfile>(&self, out: &mut [u8]) -> Result<usize, PackError> {
        if P::WIDTH != self.panel_width()
            || P::HEIGHT != self.panel_height()
            || P::ROW_BYTES != DEFAULT_ROW_BYTES
        {
            return Err(PackError::DimensionMismatch);
        }
        let required = P::ROW_BYTES.saturating_mul(P::HEIGHT);
        if out.len() < required {
            return Err(PackError::BufferTooSmall);
        }
        let mut source = 0usize;
        let mut destination = 0usize;
        let mut row = 0usize;
        while row < P::HEIGHT {
            let Some(row_data) = self
                .data
                .get(source..source.saturating_add(DEFAULT_ROW_BYTES))
            else {
                return Err(PackError::BufferTooSmall);
            };
            let Some(out_row) = out.get_mut(destination..destination.saturating_add(P::ROW_BYTES))
            else {
                return Err(PackError::BufferTooSmall);
            };
            P::pack_row(row_data, out_row)?;
            source = source.saturating_add(DEFAULT_ROW_BYTES);
            destination = destination.saturating_add(P::ROW_BYTES);
            row = row.saturating_add(1);
        }
        Ok(required)
    }

    /// Pack the physical frame for the default [`Waveshare75V2`] panel.
    pub fn pack_default(&self, out: &mut [u8]) -> Result<usize, PackError> {
        self.pack::<Waveshare75V2>(out)
    }

    fn map(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let width = i32::try_from(self.width()).ok()?;
        let height = i32::try_from(self.height()).ok()?;
        if x < 0 || y < 0 || x >= width || y >= height {
            return None;
        }
        let panel_width = i32::try_from(self.panel_width()).ok()?;
        let panel_height = i32::try_from(self.panel_height()).ok()?;
        let (px, py) = match self.rotation {
            Rotation::None => (x, y),
            Rotation::Quarter => (panel_width.saturating_sub(1).saturating_sub(y), x),
            Rotation::Half => (
                panel_width.saturating_sub(1).saturating_sub(x),
                panel_height.saturating_sub(1).saturating_sub(y),
            ),
            Rotation::ThreeQuarter => (y, panel_height.saturating_sub(1).saturating_sub(x)),
        };
        Some((usize::try_from(px).ok()?, usize::try_from(py).ok()?))
    }
}

fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
#[path = "framebuffer_ut.rs"]
mod tests;
