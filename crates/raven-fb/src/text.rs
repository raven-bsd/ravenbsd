// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Monochrome text blitter, scrolling and cursor for a linear display buffer.
//!
//! The console owns the text cursor and the dirty-row accounting; the caller owns the pixels and
//! performs any cache maintenance for the returned rows. A row index is a pixel row of the
//! screen, so the kernel can clean only the display-buffer bytes it changed.

use super::font::{FONT_HEIGHT, FONT_WIDTH, glyph};
use super::pixel::{Color, PixelFormat};

/// A caller-owned linear display buffer.
#[derive(Debug)]
pub struct ScreenBuffer<'a> {
    /// Display-buffer bytes, starting at the top-left pixel.
    pub pixels: &'a mut [u8],
    /// Visible width in pixels.
    pub width: u32,
    /// Visible height in pixels.
    pub height: u32,
    /// Bytes between the start of one row and the next.
    pub pitch: u32,
    /// Pixel encoding.
    pub format: PixelFormat,
}

/// A console or screen failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    /// The screen is smaller than the console geometry or its stride.
    ScreenTooSmall,
}

/// Inclusive pixel-row range changed by a console operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRows {
    /// First changed pixel row.
    pub first: u32,
    /// Last changed pixel row, inclusive.
    pub last: u32,
}

impl DirtyRows {
    fn single(row: u32) -> Self {
        Self {
            first: row,
            last: row,
        }
    }

    fn include(&mut self, row: u32) {
        self.first = self.first.min(row);
        self.last = self.last.max(row);
    }
}

/// An 8x16 monochrome text console over a [`ScreenBuffer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextConsole {
    columns: u32,
    rows: u32,
    column: u32,
    row: u32,
}

impl TextConsole {
    /// Create a console for `screen`, or fail when it cannot hold one cell.
    pub fn new(screen: &ScreenBuffer<'_>) -> Result<Self, TextError> {
        let columns = screen.width / FONT_WIDTH as u32;
        let rows = screen.height / FONT_HEIGHT as u32;
        if columns == 0 || rows == 0 {
            return Err(TextError::ScreenTooSmall);
        }
        check_screen(screen)?;
        Ok(Self {
            columns,
            rows,
            column: 0,
            row: 0,
        })
    }

    /// Number of text columns.
    #[must_use]
    pub const fn columns(&self) -> u32 {
        self.columns
    }

    /// Number of text rows.
    #[must_use]
    pub const fn rows(&self) -> u32 {
        self.rows
    }

    /// Current cursor cell `(column, row)`.
    #[must_use]
    pub const fn cursor(&self) -> (u32, u32) {
        (self.column, self.row)
    }

    /// Clear the whole screen, reset the cursor and draw it at the top-left cell.
    pub fn clear(&mut self, screen: &mut ScreenBuffer<'_>) -> Result<DirtyRows, TextError> {
        check_screen(screen)?;
        let color = screen.format.encode(Color::Black);
        fill(screen, color);
        let dirty = DirtyRows {
            first: 0,
            last: screen.height.saturating_sub(1),
        };
        self.column = 0;
        self.row = 0;
        draw_cursor(screen, self.column, self.row);
        Ok(dirty)
    }

    /// Write bytes to the console; returns the pixel rows that changed.
    pub fn write(
        &mut self,
        screen: &mut ScreenBuffer<'_>,
        bytes: &[u8],
    ) -> Result<DirtyRows, TextError> {
        check_screen(screen)?;
        if bytes.is_empty() {
            return Ok(DirtyRows::single(self.cursor_row()));
        }
        let mut dirty = self.erase_cursor(screen);
        for &byte in bytes {
            match byte {
                b'\r' => self.column = 0,
                b'\n' => {
                    self.column = 0;
                    let row = self.advance_row(screen, &mut dirty);
                    dirty.include(row.saturating_mul(FONT_HEIGHT as u32));
                }
                b'\t' => {
                    let spaces = 4 - (self.column % 4);
                    for _ in 0..spaces {
                        self.put_character(screen, b' ', &mut dirty);
                    }
                }
                0x08 => {
                    if self.column > 0 {
                        self.column -= 1;
                    }
                    self.clear_cell(screen, self.column, self.row, &mut dirty);
                }
                0x20..=0x7e => self.put_character(screen, byte, &mut dirty),
                _ => {}
            }
        }
        draw_cursor(screen, self.column, self.row);
        dirty.include(self.cursor_row());
        dirty.include(self.cursor_row().saturating_add(FONT_HEIGHT as u32 - 1));
        Ok(dirty)
    }

    fn put_character(&mut self, screen: &mut ScreenBuffer<'_>, byte: u8, dirty: &mut DirtyRows) {
        self.draw_cell(screen, self.column, self.row, byte, dirty);
        self.column += 1;
        if self.column >= self.columns {
            self.column = 0;
            self.advance_row(screen, dirty);
        }
    }

    fn advance_row(&mut self, screen: &mut ScreenBuffer<'_>, dirty: &mut DirtyRows) -> u32 {
        self.row += 1;
        if self.row >= self.rows {
            self.scroll(screen, dirty);
            self.row = self.rows.saturating_sub(1);
        }
        self.row
    }

    fn scroll(&self, screen: &mut ScreenBuffer<'_>, dirty: &mut DirtyRows) {
        let height = FONT_HEIGHT;
        let pitch = usize::try_from(screen.pitch).unwrap_or(0);
        let rows = usize::try_from(self.rows).unwrap_or(0);
        if height == 0 || pitch == 0 || rows == 0 {
            return;
        }
        let src = height.saturating_mul(pitch);
        let len = rows
            .saturating_sub(1)
            .saturating_mul(height)
            .saturating_mul(pitch);
        if src >= screen.pixels.len() || src.saturating_add(len) > screen.pixels.len() {
            return;
        }
        screen.pixels.copy_within(src..src + len, 0);
        let color = screen.format.encode(Color::Black);
        let start = rows.saturating_sub(1).saturating_mul(height);
        let y = u32::try_from(start).unwrap_or(screen.height);
        fill_rows(screen, y, screen.height, color);
        dirty.first = 0;
        dirty.last = screen.height.saturating_sub(1);
    }

    fn draw_cell(
        &self,
        screen: &mut ScreenBuffer<'_>,
        column: u32,
        row: u32,
        byte: u8,
        dirty: &mut DirtyRows,
    ) {
        let glyph = glyph(byte);
        let left = column.saturating_mul(FONT_WIDTH as u32);
        let top = row.saturating_mul(FONT_HEIGHT as u32);
        for (offset, bits) in glyph.data.iter().enumerate() {
            let y = top.saturating_add(u32::try_from(offset).unwrap_or(0));
            for x in 0..FONT_WIDTH {
                let mask = 0x80u8 >> x;
                let color = if *bits & mask != 0 {
                    Color::White
                } else {
                    Color::Black
                };
                set_pixel(
                    screen,
                    left.saturating_add(u32::try_from(x).unwrap_or(0)),
                    y,
                    color,
                );
            }
        }
        dirty.include(top);
        dirty.include(top.saturating_add(FONT_HEIGHT as u32 - 1));
    }

    fn clear_cell(
        &self,
        screen: &mut ScreenBuffer<'_>,
        column: u32,
        row: u32,
        dirty: &mut DirtyRows,
    ) {
        let left = column.saturating_mul(FONT_WIDTH as u32);
        let top = row.saturating_mul(FONT_HEIGHT as u32);
        for y in top..top.saturating_add(FONT_HEIGHT as u32) {
            for x in left..left.saturating_add(FONT_WIDTH as u32) {
                set_pixel(screen, x, y, Color::Black);
            }
        }
        dirty.include(top);
        dirty.include(top.saturating_add(FONT_HEIGHT as u32 - 1));
    }

    fn erase_cursor(&self, screen: &mut ScreenBuffer<'_>) -> DirtyRows {
        let left = self.column.saturating_mul(FONT_WIDTH as u32);
        let top = self.row.saturating_mul(FONT_HEIGHT as u32);
        let first = top.saturating_add(FONT_HEIGHT as u32 - 2);
        for y in first..top.saturating_add(FONT_HEIGHT as u32) {
            for x in left..left.saturating_add(FONT_WIDTH as u32) {
                set_pixel(screen, x, y, Color::Black);
            }
        }
        DirtyRows {
            first,
            last: top.saturating_add(FONT_HEIGHT as u32 - 1),
        }
    }

    fn cursor_row(&self) -> u32 {
        self.row.saturating_mul(FONT_HEIGHT as u32)
    }
}

fn draw_cursor(screen: &mut ScreenBuffer<'_>, column: u32, row: u32) {
    let left = column.saturating_mul(FONT_WIDTH as u32);
    let top = row
        .saturating_mul(FONT_HEIGHT as u32)
        .saturating_add(FONT_HEIGHT as u32 - 2);
    for y in top..top.saturating_add(2) {
        for x in left..left.saturating_add(FONT_WIDTH as u32) {
            set_pixel(screen, x, y, Color::White);
        }
    }
}

fn check_screen(screen: &ScreenBuffer<'_>) -> Result<(), TextError> {
    let bytes = screen.format.bytes_per_pixel();
    let stride = usize::try_from(screen.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes))
        .ok_or(TextError::ScreenTooSmall)?;
    let pitch = usize::try_from(screen.pitch).map_err(|_| TextError::ScreenTooSmall)?;
    if pitch < stride || !pitch.is_multiple_of(bytes) {
        return Err(TextError::ScreenTooSmall);
    }
    let required = usize::try_from(screen.pitch)
        .ok()
        .and_then(|pitch| pitch.checked_mul(usize::try_from(screen.height).ok()?))
        .ok_or(TextError::ScreenTooSmall)?;
    if required > screen.pixels.len() {
        return Err(TextError::ScreenTooSmall);
    }
    Ok(())
}

fn set_pixel(screen: &mut ScreenBuffer<'_>, x: u32, y: u32, color: Color) {
    if x >= screen.width || y >= screen.height {
        return;
    }
    let bytes = screen.format.bytes_per_pixel();
    let offset = usize::try_from(y)
        .ok()
        .and_then(|row| row.checked_mul(usize::try_from(screen.pitch).ok()?))
        .and_then(|base| {
            usize::try_from(x)
                .ok()
                .and_then(|column| column.checked_mul(bytes))
                .and_then(|column| base.checked_add(column))
        });
    let Some(offset) = offset else {
        return;
    };
    let value = screen.format.encode(color);
    let end = offset.saturating_add(bytes);
    let Some(slot) = screen.pixels.get_mut(offset..end) else {
        return;
    };
    match bytes {
        4 => slot.copy_from_slice(&value.to_le_bytes()),
        2 => slot.copy_from_slice(&(value as u16).to_le_bytes()),
        _ => slot[0] = value as u8,
    }
}

fn fill(screen: &mut ScreenBuffer<'_>, value: u32) {
    fill_rows(screen, 0, screen.height, value);
}

fn fill_rows(screen: &mut ScreenBuffer<'_>, first: u32, last: u32, value: u32) {
    let bytes = screen.format.bytes_per_pixel();
    let pitch = usize::try_from(screen.pitch).unwrap_or(0);
    let top = usize::try_from(first).unwrap_or(0);
    let bottom = usize::try_from(last).unwrap_or(0);
    let start = top.saturating_mul(pitch);
    let end = bottom.saturating_mul(pitch);
    if start >= end || end > screen.pixels.len() {
        return;
    }
    for row in screen.pixels[start..end].chunks_mut(pitch) {
        for pixel in row.chunks_mut(bytes) {
            match bytes {
                4 => pixel.copy_from_slice(&value.to_le_bytes()),
                2 => pixel.copy_from_slice(&(value as u16).to_le_bytes()),
                _ => pixel[0] = value as u8,
            }
        }
    }
}

#[cfg(test)]
#[path = "text_ut.rs"]
mod tests;
