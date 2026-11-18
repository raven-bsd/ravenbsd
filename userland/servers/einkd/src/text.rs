// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ASCII text layout for E-Ink pages.
//!
//! The renderer clears the framebuffer, wraps at the panel width, honours `\n`, applies left,
//! centre or right alignment per visual line, clips at the panel edge and can scale each glyph
//! 1x or 2x. It is byte-oriented ASCII: `Font::glyph` decides the replacement for a non-printable
//! byte; [`BasicFont`](crate::font::BasicFont) substitutes `?`. Unsupported glyphs from a custom
//! font are skipped, not a panic.

use raven_eink::{Framebuffer, Glyph};

use crate::content::Font;

/// Horizontal alignment for each visual line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Start at the left margin.
    #[default]
    Left,
    /// Centre the line in the panel width.
    Center,
    /// End at the right margin.
    Right,
}

/// Text rendering options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextStyle {
    /// Horizontal alignment.
    pub align: Align,
    /// Glyph scale: 1 or 2.
    pub scale: u8,
    /// Left/right/top/bottom margin in pixels.
    pub margin: i32,
    /// Extra pixels between visual lines.
    pub line_gap: i32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            align: Align::Left,
            scale: 1,
            margin: 16,
            line_gap: 2,
        }
    }
}

/// Result of one text render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Number of visual lines drawn.
    pub lines: usize,
    /// Number of glyphs drawn (including substitute glyphs).
    pub glyphs: usize,
    /// True when a glyph or line was clipped at the panel edge.
    pub clipped: bool,
}

/// Why a text layout could not be produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Only scale 1 and 2 are supported.
    UnsupportedScale,
}

/// Render `text` into `frame`, clearing it first.
pub fn render_text<F: Font>(
    frame: &mut Framebuffer,
    text: &str,
    font: &F,
    style: TextStyle,
) -> Result<Layout, LayoutError> {
    if style.scale != 1 && style.scale != 2 {
        return Err(LayoutError::UnsupportedScale);
    }
    frame.clear(false);
    if text.is_empty() {
        return Ok(Layout::default());
    }

    let scale = i32::from(style.scale);
    let margin = style.margin.max(0);
    let available = (frame.width() as i32).saturating_sub(margin.saturating_mul(2));
    let line_height = font
        .glyph(b'A')
        .map_or(8, |glyph| i32::try_from(glyph.height).unwrap_or(8))
        .saturating_mul(scale)
        .saturating_add(style.line_gap.max(0))
        .max(1);
    let mut layout = Layout::default();
    let mut y = margin;

    for line in text.split('\n') {
        if line.is_empty() {
            if y >= frame.height() as i32 {
                layout.clipped = true;
                break;
            }
            layout.lines = layout.lines.saturating_add(1);
            y = y.saturating_add(line_height);
            continue;
        }
        let mut start = 0usize;
        while start < line.len() {
            if y >= frame.height() as i32 {
                layout.clipped = true;
                break;
            }
            let mut end = start;
            let mut width = 0i32;
            while end < line.len() {
                let glyph_width = glyph_width(font, line.as_bytes()[end], scale);
                if end > start && width.saturating_add(glyph_width) > available {
                    break;
                }
                width = width.saturating_add(glyph_width);
                end = end.saturating_add(1);
            }
            if end == start {
                end = start.saturating_add(1);
                width = glyph_width(font, line.as_bytes()[start], scale);
            }
            let x_start = match style.align {
                Align::Left => margin,
                Align::Center => margin.saturating_add(available.saturating_sub(width) / 2),
                Align::Right => margin.saturating_add(available.saturating_sub(width)),
            };
            let mut x = x_start;
            let mut index = start;
            while index < end {
                let character = line.as_bytes().get(index).copied().unwrap_or(b'?');
                if let Some(glyph) = font.glyph(character) {
                    let width = i32::try_from(glyph.width)
                        .unwrap_or(i32::MAX)
                        .saturating_mul(scale);
                    let height = i32::try_from(glyph.height)
                        .unwrap_or(i32::MAX)
                        .saturating_mul(scale);
                    if x.saturating_add(width) > frame.width() as i32
                        || y.saturating_add(height) > frame.height() as i32
                    {
                        layout.clipped = true;
                    }
                    draw_scaled(frame, x, y, &glyph, scale);
                    layout.glyphs = layout.glyphs.saturating_add(1);
                    x = x.saturating_add(width);
                }
                index = index.saturating_add(1);
            }
            if x > frame.width() as i32 {
                layout.clipped = true;
            }
            layout.lines = layout.lines.saturating_add(1);
            y = y.saturating_add(line_height);
            start = end;
        }
    }
    Ok(layout)
}

fn glyph_width<F: Font>(font: &F, character: u8, scale: i32) -> i32 {
    font.glyph(character).map_or(0, |glyph| {
        i32::try_from(glyph.width)
            .unwrap_or(i32::MAX)
            .saturating_mul(scale)
    })
}

fn draw_scaled(frame: &mut Framebuffer, x: i32, y: i32, glyph: &Glyph<'_>, scale: i32) {
    let mut row = 0usize;
    while row < glyph.height {
        let bits = glyph.data.get(row).copied().unwrap_or(0);
        let mut column = 0usize;
        while column < glyph.width {
            if bits & (0x80 >> (column % 8)) != 0 {
                let px = x.saturating_add(
                    i32::try_from(column)
                        .unwrap_or(i32::MAX)
                        .saturating_mul(scale),
                );
                let py =
                    y.saturating_add(i32::try_from(row).unwrap_or(i32::MAX).saturating_mul(scale));
                let mut sy = 0i32;
                while sy < scale {
                    let mut sx = 0i32;
                    while sx < scale {
                        let _ = frame.set_pixel(px.saturating_add(sx), py.saturating_add(sy), true);
                        sx = sx.saturating_add(1);
                    }
                    sy = sy.saturating_add(1);
                }
            }
            column = column.saturating_add(1);
        }
        row = row.saturating_add(1);
    }
}

#[cfg(test)]
#[path = "text_ut.rs"]
mod tests;
