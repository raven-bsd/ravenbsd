// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party Pi 4 E-Ink core.
//!
//! This crate is the hardware-independent half of the 0.1.0 Pi 4 E-Ink use case:
//!
//! * an 800x480 1-bit framebuffer with rotation, clear, pixel, line, rectangle and MSB-first
//!   glyph/bitmap blitting;
//! * a [`PanelProfile`] row-packing seam, with the Waveshare 7.5-inch V2-class UC8179 profile;
//! * a [`Uc8179`] command encoder driven by [`Transport`] and [`Delay`] traits, so the complete
//!   reset/power/configuration/frame/refresh/sleep sequence is host-testable with a recording
//!   mock.
//!
//! The crate does not open SPI, touch a GPIO or claim a hardware pass. The Pi 4 SPI/GPIO
//! transport and the fetch/render service are E2/E3 work; the design and wiring plan are in
//! `docs/design/pi4-eink.md`.
//!
//! The framebuffer uses `1` = black; [`Waveshare75V2`] inverts rows to the controller's
//! `1` = white RAM convention while packing.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod framebuffer;
mod panel;
mod profile;

pub use framebuffer::{Framebuffer, Glyph, Rotation};
pub use panel::{
    CMD_BOOSTER_SOFT_START, CMD_DATA_START, CMD_DEEP_SLEEP, CMD_DISPLAY_REFRESH, CMD_LUT_BB,
    CMD_LUT_BW, CMD_LUT_VCOM, CMD_LUT_WB, CMD_LUT_WW, CMD_PANEL_SETTING, CMD_PLL_CONTROL,
    CMD_POWER_OFF, CMD_POWER_ON, CMD_POWER_SETTING, CMD_RESOLUTION, CMD_TCON, CMD_VCOM, Delay,
    Error, LUT_BB, LUT_BW, LUT_VCOM, LUT_WB, LUT_WW, MAX_BUSY_POLLS, POWER_ON_MS, REFRESH_MS,
    RESET_MS, Transport, Uc8179,
};
pub use profile::{
    DEFAULT_FRAME_BYTES, DEFAULT_HEIGHT, DEFAULT_ROW_BYTES, DEFAULT_WIDTH, PackError, PanelProfile,
    Waveshare75V2,
};
