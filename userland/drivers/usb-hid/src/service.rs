// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Keyboard service: boot reports in, console bytes out.

use raven_usb_hid::{BootKeyboard, Error, TransferRing};

use crate::console::ConsoleInput;

/// A boot keyboard bound to a transfer ring and a console input queue.
pub struct KeyboardService<R: TransferRing> {
    keyboard: BootKeyboard<R>,
    console: ConsoleInput,
}

impl<R: TransferRing> KeyboardService<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            keyboard: BootKeyboard::new(ring),
            console: ConsoleInput::new(),
        }
    }

    /// Select boot protocol and validate the report descriptor.
    ///
    /// # Errors
    ///
    /// The descriptor/transport errors from [`BootKeyboard::init`].
    pub fn init(&mut self, report_descriptor: &[u8]) -> Result<(), Error> {
        self.keyboard.init(report_descriptor).map(|_| ())
    }

    /// Decode one completed report and queue the pressed-key ASCII.
    ///
    /// Releases, modifiers and non-ASCII keys do not produce console bytes. Returns the number of
    /// bytes queued (zero when no report completed or the report carried no ASCII).
    ///
    /// # Errors
    ///
    /// [`Error::NotInitialized`] before [`Self::init`], or a transport/report error.
    pub fn poll(&mut self) -> Result<usize, Error> {
        let Some(events) = self.keyboard.poll()? else {
            return Ok(0);
        };
        let mut queued = 0usize;
        for event in events.iter() {
            if event.pressed
                && let Some(ascii) = event.ascii
                && self.console.push(ascii)
            {
                queued = queued.saturating_add(1);
            }
        }
        Ok(queued)
    }

    /// Drain the console input queue.
    pub fn read_console(&mut self, out: &mut [u8]) -> usize {
        self.console.read(out)
    }

    /// Bytes waiting in the console queue.
    #[must_use]
    pub fn console_len(&self) -> usize {
        self.console.len()
    }

    /// The console queue.
    #[must_use]
    pub const fn console(&self) -> &ConsoleInput {
        &self.console
    }

    /// The transport, for the xHCI binding.
    #[must_use]
    pub const fn ring(&self) -> &R {
        self.keyboard.ring()
    }

    /// The keyboard model.
    #[must_use]
    pub const fn keyboard(&self) -> &BootKeyboard<R> {
        &self.keyboard
    }
}

#[cfg(test)]
#[path = "service_ut.rs"]
mod tests;
