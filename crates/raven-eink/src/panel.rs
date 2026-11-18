// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! UC8179 / Waveshare 7.5-inch V2-class panel command encoder.
//!
//! The encoder is deliberately transport-agnostic: [`Transport`] supplies the SPI command/data
//! path, reset line and BUSY input; [`Delay`] supplies the panel timing. A mock transport records
//! the exact byte sequence on the host. No hardware pass is claimed here; the Pi 4 wiring and
//! measured refresh time are the E2/E3 hardware gate.
//!
//! The command values and full-refresh waveform tables are the vendor's UC8179 documentation for
//! the 800x480 7.5-inch V2 panel. They are constants, not ported source.

use crate::framebuffer::Framebuffer;
use crate::profile::{DEFAULT_FRAME_BYTES, PackError, Waveshare75V2};

/// UC8179 command: panel setting.
pub const CMD_PANEL_SETTING: u8 = 0x00;
/// UC8179 command: power setting.
pub const CMD_POWER_SETTING: u8 = 0x01;
/// UC8179 command: power off.
pub const CMD_POWER_OFF: u8 = 0x02;
/// UC8179 command: power on.
pub const CMD_POWER_ON: u8 = 0x04;
/// UC8179 command: booster soft start.
pub const CMD_BOOSTER_SOFT_START: u8 = 0x06;
/// UC8179 command: deep sleep.
pub const CMD_DEEP_SLEEP: u8 = 0x07;
/// UC8179 command: start transmitting frame data.
pub const CMD_DATA_START: u8 = 0x10;
/// UC8179 command: display refresh.
pub const CMD_DISPLAY_REFRESH: u8 = 0x12;
/// UC8179 command: LUT for VCOM.
pub const CMD_LUT_VCOM: u8 = 0x20;
/// UC8179 command: LUT for white-to-white.
pub const CMD_LUT_WW: u8 = 0x21;
/// UC8179 command: LUT for black-to-white.
pub const CMD_LUT_BW: u8 = 0x22;
/// UC8179 command: LUT for white-to-black.
pub const CMD_LUT_WB: u8 = 0x23;
/// UC8179 command: LUT for black-to-black.
pub const CMD_LUT_BB: u8 = 0x24;
/// UC8179 command: PLL control.
pub const CMD_PLL_CONTROL: u8 = 0x30;
/// UC8179 command: VCOM and data interval.
pub const CMD_VCOM: u8 = 0x50;
/// UC8179 command: TCON setting.
pub const CMD_TCON: u8 = 0x60;
/// UC8179 command: resolution setting.
pub const CMD_RESOLUTION: u8 = 0x61;

/// Panel transaction or timing failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The transport rejected a command/data/reset operation.
    Transport,
    /// BUSY did not release within the bounded poll window.
    BusyTimeout,
    /// The framebuffer could not be packed for this panel.
    Pack(PackError),
}

impl From<PackError> for Error {
    fn from(error: PackError) -> Self {
        Self::Pack(error)
    }
}

/// SPI command/data, reset and BUSY seam.
///
/// `command` asserts DC low for one byte; `data` asserts DC high for the slice. `reset` drives
/// the active-low reset line. `busy` returns true while the panel holds BUSY high.
pub trait Transport {
    /// Drive the active-low reset line (`true` asserts reset).
    fn reset(&mut self, asserted: bool) -> Result<(), Error>;

    /// Send one command byte.
    fn command(&mut self, command: u8) -> Result<(), Error>;

    /// Send command data.
    fn data(&mut self, data: &[u8]) -> Result<(), Error>;

    /// Read the panel BUSY line; defaults to ready for mock/loopback transports.
    fn busy(&mut self) -> Result<bool, Error> {
        Ok(false)
    }
}

/// Millisecond delay seam.
pub trait Delay {
    /// Sleep for at least `milliseconds`.
    fn delay_ms(&mut self, milliseconds: u32);
}

/// Full-refresh VCOM LUT (vendor table).
pub const LUT_VCOM: [u8; 44] = [
    0x00, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x60, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x13, 0x0a, 0x0b, 0x0a, 0x08, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
/// Full-refresh white-to-white LUT (vendor table).
pub const LUT_WW: [u8; 42] = [
    0x40, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x90, 0x14, 0x14, 0x00, 0x00, 0x01, 0x10, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x13, 0x0a, 0x0b, 0x0a, 0x08, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
/// Full-refresh black-to-white LUT (vendor table).
pub const LUT_BW: [u8; 42] = [
    0x40, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x90, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x13, 0x0a, 0x0b, 0x0a, 0x08, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
/// Full-refresh white-to-black LUT (vendor table).
pub const LUT_WB: [u8; 42] = [
    0x80, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x90, 0x14, 0x14, 0x00, 0x00, 0x01, 0x10, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x13, 0x0a, 0x0b, 0x0a, 0x08, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
/// Full-refresh black-to-black LUT (vendor table).
pub const LUT_BB: [u8; 42] = [
    0x80, 0x0a, 0x00, 0x00, 0x00, 0x01, 0x90, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x13, 0x0a, 0x0b, 0x0a, 0x08, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x3c, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Maximum BUSY polls before a timeout (10 ms apart, 10 s total).
pub const MAX_BUSY_POLLS: usize = 1000;
/// Reset and power timing in milliseconds.
pub const RESET_MS: u32 = 10;
/// Power-on settling delay in milliseconds.
pub const POWER_ON_MS: u32 = 100;
/// Full refresh settling delay in milliseconds.
pub const REFRESH_MS: u32 = 100;

/// UC8179 full-refresh encoder.
pub struct Uc8179<T: Transport, D: Delay> {
    transport: T,
    delay: D,
}

impl<T: Transport, D: Delay> Uc8179<T, D> {
    /// Build the encoder around a transport and delay source.
    pub const fn new(transport: T, delay: D) -> Self {
        Self { transport, delay }
    }

    /// Reset, power on and load the full-refresh panel configuration.
    ///
    /// The caller selects the full-refresh waveform; partial refresh is deliberately out of E1.
    pub fn init(&mut self) -> Result<(), Error> {
        self.transport.reset(true)?;
        self.delay.delay_ms(RESET_MS);
        self.transport.reset(false)?;
        self.delay.delay_ms(RESET_MS);
        self.wait_ready()?;

        self.command_data(CMD_PANEL_SETTING, &[0x1f, 0x0f])?;
        self.command_data(CMD_POWER_SETTING, &[0x07, 0x07, 0x3f, 0x3f])?;
        self.command(CMD_POWER_ON)?;
        self.delay.delay_ms(POWER_ON_MS);
        self.wait_ready()?;
        self.command_data(CMD_BOOSTER_SOFT_START, &[0x17, 0x17, 0x28, 0x17])?;
        self.command_data(CMD_PLL_CONTROL, &[0x3c])?;
        self.command_data(CMD_RESOLUTION, &[0x03, 0x20, 0x01, 0xe0])?;
        self.command_data(CMD_VCOM, &[0x11, 0x07])?;
        self.command_data(CMD_LUT_VCOM, &LUT_VCOM)?;
        self.command_data(CMD_LUT_WW, &LUT_WW)?;
        self.command_data(CMD_LUT_BW, &LUT_BW)?;
        self.command_data(CMD_LUT_WB, &LUT_WB)?;
        self.command_data(CMD_LUT_BB, &LUT_BB)?;
        self.command_data(CMD_TCON, &[0x22])?;
        Ok(())
    }

    /// Send a complete frame and request a full refresh.
    ///
    /// The framebuffer is packed for the default [`Waveshare75V2`] row format; a full refresh
    /// takes the panel's refresh time and is therefore called after login by the E3 service, not
    /// on the boot/login path.
    pub fn display(&mut self, framebuffer: &Framebuffer) -> Result<(), Error> {
        let mut packed = [0u8; DEFAULT_FRAME_BYTES];
        let length = framebuffer.pack::<Waveshare75V2>(&mut packed)?;
        self.command(CMD_DATA_START)?;
        self.transport
            .data(packed.get(..length).ok_or(Error::Transport)?)?;
        self.command(CMD_DISPLAY_REFRESH)?;
        self.delay.delay_ms(REFRESH_MS);
        self.wait_ready()
    }

    /// Power off and enter deep sleep.
    pub fn sleep(&mut self) -> Result<(), Error> {
        self.command(CMD_POWER_OFF)?;
        self.delay.delay_ms(POWER_ON_MS);
        self.command(CMD_DEEP_SLEEP)?;
        self.transport.data(&[0xa5])
    }

    /// Wait for BUSY to release, bounded by [`MAX_BUSY_POLLS`].
    pub fn wait_ready(&mut self) -> Result<(), Error> {
        let mut polls = 0usize;
        while polls < MAX_BUSY_POLLS {
            if !self.transport.busy()? {
                return Ok(());
            }
            self.delay.delay_ms(10);
            polls = polls.saturating_add(1);
        }
        Err(Error::BusyTimeout)
    }

    fn command(&mut self, command: u8) -> Result<(), Error> {
        self.transport.command(command)
    }

    fn command_data(&mut self, command: u8, data: &[u8]) -> Result<(), Error> {
        self.command(command)?;
        self.transport.data(data)
    }
}

#[cfg(test)]
#[path = "panel_ut.rs"]
mod tests;
