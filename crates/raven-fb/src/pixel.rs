// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Monochrome pixel encoding for the firmware display-buffer depths we support.
//!
//! The console only needs black and white, so a depth's colour layout reduces to "all bits set"
//! (white) or "all bits clear" (black). The 16-bit and 8-bit forms are still modelled explicitly
//! because the firmware may return them even when `config.txt` asks for 32 bpp.

/// A logical console colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Background/foreground black.
    Black,
    /// Foreground/background white.
    White,
}

/// A linear display-buffer pixel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// 32 bits per pixel, one padding byte plus 8 bits per primary.
    Rgb888,
    /// 16 bits per pixel, 5-6-5 primaries.
    Rgb565,
    /// 8 bits per pixel, indexed by the firmware palette.
    Indexed8,
}

impl PixelFormat {
    /// Map a firmware depth to a supported format.
    #[must_use]
    pub const fn from_depth(depth: u32) -> Option<Self> {
        match depth {
            32 => Some(Self::Rgb888),
            16 => Some(Self::Rgb565),
            8 => Some(Self::Indexed8),
            _ => None,
        }
    }

    /// Bytes occupied by one pixel.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb888 => 4,
            Self::Rgb565 => 2,
            Self::Indexed8 => 1,
        }
    }

    /// The packed value written for `color`.
    #[must_use]
    pub const fn encode(self, color: Color) -> u32 {
        match (self, color) {
            (_, Color::Black) => 0,
            (Self::Rgb888, Color::White) => 0x00ff_ffff,
            (Self::Rgb565, Color::White) => 0xffff,
            (Self::Indexed8, Color::White) => 0xff,
        }
    }
}

#[cfg(test)]
#[path = "pixel_ut.rs"]
mod tests;
