// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Keyboard service: boot reports in, console bytes out.

use raven_usb_hid::{BootKeyboard, Error, Layout, TransferRing};

use crate::console::ConsoleInput;
use crate::repeat::{Repeat, RepeatSettings};

/// A boot keyboard bound to a transfer ring and a console input queue.
pub struct KeyboardService<R: TransferRing> {
    keyboard: BootKeyboard<R>,
    console: ConsoleInput,
    repeat: Repeat,
}

impl<R: TransferRing> KeyboardService<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            keyboard: BootKeyboard::new(ring),
            console: ConsoleInput::new(),
            repeat: Repeat::new(RepeatSettings::DEFAULT),
        }
    }

    /// Wrap a transport with a selected keymap layout.
    pub const fn with_layout(ring: R, layout: Layout) -> Self {
        Self {
            keyboard: BootKeyboard::with_layout(ring, layout),
            console: ConsoleInput::new(),
            repeat: Repeat::new(RepeatSettings::DEFAULT),
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
    /// This is the S62-compatible entry point: it uses time 0 for the repeat deadline, so a
    /// caller that polls without a clock never sees repeats. Use [`Self::poll_at`] and
    /// [`Self::tick`] for auto-repeat.
    ///
    /// # Errors
    ///
    /// [`Error::NotInitialized`] before [`Self::init`], or a transport/report error.
    pub fn poll(&mut self) -> Result<usize, Error> {
        self.poll_at(0)
    }

    /// Decode one report at `now_ns` and queue its pressed-key ASCII.
    ///
    /// Releases, modifiers and non-ASCII keys do not produce console bytes; a pressed ASCII key
    /// becomes the repeat key. Returns the number of bytes queued.
    ///
    /// # Errors
    ///
    /// [`Error::NotInitialized`] before [`Self::init`], or a transport/report error.
    pub fn poll_at(&mut self, now_ns: u64) -> Result<usize, Error> {
        let Some(events) = self.keyboard.poll()? else {
            return Ok(0);
        };
        let mut queued = 0usize;
        for event in events.iter() {
            if event.pressed {
                if let Some(ascii) = event.ascii
                    && self.console.push(ascii)
                {
                    queued = queued.saturating_add(1);
                }
                self.repeat.press(event.usage, event.ascii, now_ns);
            } else {
                self.repeat.release(event.usage);
            }
        }
        Ok(queued)
    }

    /// Emit one auto-repeat byte when the deadline has passed.
    pub fn tick(&mut self, now_ns: u64) -> usize {
        let Some(byte) = self.repeat.tick(now_ns) else {
            return 0;
        };
        if self.console.push(byte) { 1 } else { 0 }
    }

    /// Replace the repeat rate.
    pub fn set_repeat(&mut self, settings: RepeatSettings) {
        self.repeat.set_settings(settings);
    }

    /// The repeat state.
    #[must_use]
    pub const fn repeat(&self) -> &Repeat {
        &self.repeat
    }

    /// Select the keymap layout.
    pub fn set_layout(&mut self, layout: Layout) {
        self.keyboard.set_layout(layout);
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
