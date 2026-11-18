// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Panel geometry and row packing.
//!
//! The framebuffer stores one bit per pixel (`1` = black) in panel order. A profile defines the
//! physical panel size and converts one physical row to the controller's data format. The
//! default profile is the 7.5-inch 800x480 Waveshare V2-class UC8179 panel: its RAM uses
//! `1` = white, so [`Waveshare75V2`] inverts each row while packing.

/// Default panel width in pixels.
pub const DEFAULT_WIDTH: usize = 800;
/// Default panel height in pixels.
pub const DEFAULT_HEIGHT: usize = 480;
/// Bytes in one default panel row (8 pixels per byte).
pub const DEFAULT_ROW_BYTES: usize = DEFAULT_WIDTH / 8;
/// Bytes in one default panel frame.
pub const DEFAULT_FRAME_BYTES: usize = DEFAULT_ROW_BYTES * DEFAULT_HEIGHT;

/// A panel geometry and row-packing rule.
pub trait PanelProfile {
    /// Physical panel width in pixels.
    const WIDTH: usize;
    /// Physical panel height in pixels.
    const HEIGHT: usize;
    /// Bytes in one packed panel row.
    const ROW_BYTES: usize;

    /// Pack one physical row from the framebuffer's 1-bit black convention into panel RAM.
    ///
    /// `row` and `out` must both be at least [`Self::ROW_BYTES`] long.
    fn pack_row(row: &[u8], out: &mut [u8]) -> Result<(), PackError>;
}

/// Why a row or frame could not be packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackError {
    /// The destination slice is smaller than the profile's row/frame.
    BufferTooSmall,
    /// The framebuffer geometry does not match the profile.
    DimensionMismatch,
}

/// The Waveshare 7.5-inch V2-class panel: 800x480, UC8179, `1` = white in panel RAM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Waveshare75V2;

impl PanelProfile for Waveshare75V2 {
    const WIDTH: usize = DEFAULT_WIDTH;
    const HEIGHT: usize = DEFAULT_HEIGHT;
    const ROW_BYTES: usize = DEFAULT_ROW_BYTES;

    fn pack_row(row: &[u8], out: &mut [u8]) -> Result<(), PackError> {
        if row.len() < Self::ROW_BYTES || out.len() < Self::ROW_BYTES {
            return Err(PackError::BufferTooSmall);
        }
        for (destination, source) in out.iter_mut().zip(row.iter()).take(Self::ROW_BYTES) {
            // The controller's RAM is 1 = white; the framebuffer is 1 = black.
            *destination = !source;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "profile_ut.rs"]
mod tests;
