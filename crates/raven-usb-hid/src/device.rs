// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Boot-keyboard device model behind the xHCI transfer-ring seam.
//!
//! The model is transport-agnostic: [`TransferRing`] is implemented by the xHCI adapter in Phil's
//! lane, and by the host tests here. The real enumeration and transfer-ring binding are not part
//! of this ticket; the model proves the class requests, the boot report decode and the resubmit
//! loop.

use crate::Error;
use crate::descriptor::{ReportLayout, parse_report_descriptor};
use crate::report::{BOOT_REPORT_LEN, BootReport, KeyEvents, Keyboard};
use crate::setup::SetupPacket;

/// Interrupt-IN endpoint used by the boot keyboard model.
pub const REPORT_ENDPOINT: u8 = 1;

/// The transport seam the model drives.
///
/// `interrupt_in` queues one interrupt-IN transfer for `length` bytes. `next_report` returns the
/// next completed eight-byte report, or `None` when no transfer has completed. The xHCI adapter
/// owns the transfer buffer and copies the event payload into the fixed report.
pub trait TransferRing {
    /// Submit a host-to-device control transfer.
    fn control_out(&mut self, setup: SetupPacket, data: &[u8]) -> Result<(), Error>;
    /// Submit a device-to-host control transfer; returns the received length.
    fn control_in(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, Error>;
    /// Queue an interrupt-IN transfer for `length` bytes.
    fn interrupt_in(&mut self, endpoint: u8, length: usize) -> Result<(), Error>;
    /// Take the next completed boot report.
    fn next_report(&mut self) -> Option<[u8; BOOT_REPORT_LEN]>;
}

/// A boot-protocol keyboard bound to one transfer ring.
pub struct BootKeyboard<R: TransferRing> {
    ring: R,
    keyboard: Keyboard,
    primed: bool,
}

impl<R: TransferRing> BootKeyboard<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            ring,
            keyboard: Keyboard::new(),
            primed: false,
        }
    }

    /// Select boot protocol, disable idle reporting and parse the report descriptor.
    ///
    /// The caller fetches the report descriptor with
    /// [`TransferRing::control_in`] and [`SetupPacket::get_descriptor`]; the model validates that
    /// it is an eight-byte boot keyboard before priming the first interrupt-IN transfer.
    ///
    /// # Errors
    ///
    /// A transport error, a malformed descriptor, or [`Error::NotAKeyboard`].
    pub fn init(&mut self, report_descriptor: &[u8]) -> Result<ReportLayout, Error> {
        self.ring.control_out(SetupPacket::set_protocol(0), &[])?;
        self.ring.control_out(SetupPacket::set_idle(0, 0), &[])?;
        let layout = parse_report_descriptor(report_descriptor)?;
        if !layout.is_boot_keyboard() {
            return Err(Error::NotAKeyboard);
        }
        self.ring.interrupt_in(REPORT_ENDPOINT, BOOT_REPORT_LEN)?;
        self.primed = true;
        Ok(layout)
    }

    /// Consume one completed report and queue the next interrupt-IN transfer.
    ///
    /// # Errors
    ///
    /// [`Error::NotInitialized`] before [`Self::init`], a transport error, or a report shorter
    /// than eight bytes.
    pub fn poll(&mut self) -> Result<Option<KeyEvents>, Error> {
        if !self.primed {
            return Err(Error::NotInitialized);
        }
        let Some(bytes) = self.ring.next_report() else {
            return Ok(None);
        };
        let report = BootReport::from_bytes(&bytes)?;
        let events = self.keyboard.update(&report);
        self.ring.interrupt_in(REPORT_ENDPOINT, BOOT_REPORT_LEN)?;
        Ok(Some(events))
    }

    /// Send the LED output report.
    ///
    /// # Errors
    ///
    /// A transport error.
    pub fn submit_leds(&mut self, leds: u8) -> Result<(), Error> {
        self.ring
            .control_out(SetupPacket::set_report(2, 0, 1), &[leds])
    }

    /// The keyboard state.
    #[must_use]
    pub const fn keyboard(&self) -> &Keyboard {
        &self.keyboard
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
#[path = "device_ut.rs"]
mod tests;
