// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-protocol mouse reports and pointer events.
//!
//! The boot mouse report is three bytes (buttons, X, Y); wheel mice extend it with a fourth
//! signed byte. [`BootMouse`] is the transport-agnostic model behind the same [`TransferRing`]
//! seam as the keyboard: the xHCI adapter and the real enumeration remain Phil's follow-up, and
//! this module makes no hardware claim.

use crate::Error;
use crate::descriptor::{ReportLayout, parse_report_descriptor};
use crate::device::{REPORT_ENDPOINT, TransferRing};
use crate::setup::SetupPacket;

/// Maximum boot mouse report length, including the wheel byte.
pub const MOUSE_REPORT_LEN: usize = 4;
/// Minimum boot mouse report length (buttons, X, Y).
pub const MOUSE_MIN_REPORT_LEN: usize = 3;

/// Left button bit.
pub const BUTTON_LEFT: u8 = 1 << 0;
/// Right button bit.
pub const BUTTON_RIGHT: u8 = 1 << 1;
/// Middle button bit.
pub const BUTTON_MIDDLE: u8 = 1 << 2;
/// Back button bit (five-button mice).
pub const BUTTON_BACK: u8 = 1 << 3;
/// Forward button bit (five-button mice).
pub const BUTTON_FORWARD: u8 = 1 << 4;
/// Defined button bits; the remaining bits are reserved padding.
pub const BUTTON_MASK: u8 =
    BUTTON_LEFT | BUTTON_RIGHT | BUTTON_MIDDLE | BUTTON_BACK | BUTTON_FORWARD;

/// Errors from the boot mouse model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseError {
    /// The HID descriptor, report or transport layer failed.
    Hid(Error),
    /// The report descriptor does not describe a boot mouse.
    NotAMouse,
}

impl From<Error> for MouseError {
    fn from(error: Error) -> Self {
        Self::Hid(error)
    }
}

/// One boot mouse input report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseReport {
    /// Button bitmap: left, right, middle, back, forward. Reserved bits are cleared.
    pub buttons: u8,
    /// Relative X movement.
    pub dx: i8,
    /// Relative Y movement.
    pub dy: i8,
    /// Relative wheel movement; zero for a three-byte report.
    pub wheel: i8,
}

impl MouseReport {
    /// Decode a three- or four-byte report.
    ///
    /// # Errors
    ///
    /// [`Error::ShortReport`] when `data` is shorter than three bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, Error> {
        let buttons = data.first().copied().ok_or(Error::ShortReport)? & BUTTON_MASK;
        let dx = data.get(1).copied().ok_or(Error::ShortReport)? as i8;
        let dy = data.get(2).copied().ok_or(Error::ShortReport)? as i8;
        let wheel = data.get(3).copied().unwrap_or(0) as i8;
        Ok(Self {
            buttons,
            dx,
            dy,
            wheel,
        })
    }

    /// True when `button` (one of the `BUTTON_*` bits) is held.
    #[must_use]
    pub const fn button(&self, button: u8) -> bool {
        self.buttons & button != 0
    }
}

/// One decoded pointer event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerEvent {
    /// Button bitmap after the report.
    pub buttons: u8,
    /// Buttons pressed by this report.
    pub pressed: u8,
    /// Buttons released by this report.
    pub released: u8,
    /// Relative X movement.
    pub dx: i8,
    /// Relative Y movement.
    pub dy: i8,
    /// Relative wheel movement.
    pub wheel: i8,
}

impl PointerEvent {
    /// An event with no button change and no movement.
    pub const NONE: Self = Self {
        buttons: 0,
        pressed: 0,
        released: 0,
        dx: 0,
        dy: 0,
        wheel: 0,
    };

    /// True when `button` (one of the `BUTTON_*` bits) is held after the report.
    #[must_use]
    pub const fn button(&self, button: u8) -> bool {
        self.buttons & button != 0
    }

    /// True when the report carried no button change and no movement.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pressed == 0 && self.released == 0 && self.dx == 0 && self.dy == 0 && self.wheel == 0
    }
}

/// Pointer state: held buttons and accumulated relative movement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mouse {
    buttons: u8,
    x: i32,
    y: i32,
    wheel: i32,
}

impl Mouse {
    /// A mouse with no button held and no accumulated movement.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buttons: 0,
            x: 0,
            y: 0,
            wheel: 0,
        }
    }

    /// The held button bitmap.
    #[must_use]
    pub const fn buttons(&self) -> u8 {
        self.buttons
    }

    /// Accumulated relative position `(x, y)`; the consumer applies the screen mapping.
    #[must_use]
    pub const fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Accumulated wheel movement.
    #[must_use]
    pub const fn wheel(&self) -> i32 {
        self.wheel
    }

    /// Decode one report into a pointer event and update the state.
    ///
    /// The event describes the state after the report, the button transitions and the relative
    /// movement. Accumulators saturate rather than wrap.
    pub fn update(&mut self, report: &MouseReport) -> PointerEvent {
        let pressed = report.buttons & !self.buttons;
        let released = self.buttons & !report.buttons;
        self.buttons = report.buttons;
        self.x = self.x.saturating_add(i32::from(report.dx));
        self.y = self.y.saturating_add(i32::from(report.dy));
        self.wheel = self.wheel.saturating_add(i32::from(report.wheel));
        PointerEvent {
            buttons: report.buttons,
            pressed,
            released,
            dx: report.dx,
            dy: report.dy,
            wheel: report.wheel,
        }
    }
}

/// A boot-protocol mouse bound to one transfer ring.
pub struct BootMouse<R: TransferRing> {
    ring: R,
    mouse: Mouse,
    report_len: usize,
    primed: bool,
}

impl<R: TransferRing> BootMouse<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            ring,
            mouse: Mouse::new(),
            report_len: MOUSE_REPORT_LEN,
            primed: false,
        }
    }

    /// Select boot protocol, disable idle reporting and validate the report descriptor.
    ///
    /// The model validates that the descriptor is a three- or four-byte Generic Desktop mouse
    /// before priming the first interrupt-IN transfer with the descriptor's report length.
    ///
    /// # Errors
    ///
    /// A transport error, a malformed descriptor, or [`MouseError::NotAMouse`].
    pub fn init(&mut self, report_descriptor: &[u8]) -> Result<ReportLayout, MouseError> {
        self.ring.control_out(SetupPacket::set_protocol(0), &[])?;
        self.ring.control_out(SetupPacket::set_idle(0, 0), &[])?;
        let layout = parse_report_descriptor(report_descriptor)?;
        let Some(report_len) = layout.boot_mouse_report_len() else {
            return Err(MouseError::NotAMouse);
        };
        self.report_len = report_len;
        self.ring.interrupt_in(REPORT_ENDPOINT, report_len)?;
        self.primed = true;
        Ok(layout)
    }

    /// Consume one completed report and queue the next interrupt-IN transfer.
    ///
    /// # Errors
    ///
    /// [`MouseError::Hid`] with [`Error::NotInitialized`] before [`Self::init`], a transport
    /// error, or a report shorter than three bytes.
    pub fn poll(&mut self) -> Result<Option<PointerEvent>, MouseError> {
        if !self.primed {
            return Err(Error::NotInitialized.into());
        }
        let Some(bytes) = self.ring.next_report() else {
            return Ok(None);
        };
        let data = bytes.get(..self.report_len).unwrap_or(&bytes);
        let report = MouseReport::from_bytes(data)?;
        let event = self.mouse.update(&report);
        self.ring.interrupt_in(REPORT_ENDPOINT, self.report_len)?;
        Ok(Some(event))
    }

    /// The active report length selected by the descriptor.
    #[must_use]
    pub const fn report_len(&self) -> usize {
        self.report_len
    }

    /// The pointer state.
    #[must_use]
    pub const fn mouse(&self) -> &Mouse {
        &self.mouse
    }

    /// The transport, for the xHCI binding.
    #[must_use]
    pub const fn ring(&self) -> &R {
        &self.ring
    }

    /// The mutable transport, for the xHCI binding.
    pub const fn ring_mut(&mut self) -> &mut R {
        &mut self.ring
    }
}

#[cfg(test)]
#[path = "mouse_ut.rs"]
mod tests;
