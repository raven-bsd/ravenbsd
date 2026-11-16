// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Keyboard service: boot reports in, console bytes out.

use raven_usb_hid::{BootKeyboard, Error, KeyEvent, KeyEvents, Keyboard, Layout, TransferRing};

use crate::console::ConsoleInput;
use crate::repeat::{Repeat, RepeatSettings};

/// Maximum held usages tracked for a detach release: six key slots plus eight modifiers.
const MAX_HELD: usize = 14;

/// A boot keyboard bound to a transfer ring and a console input queue.
pub struct KeyboardService<R: TransferRing> {
    keyboard: BootKeyboard<R>,
    console: ConsoleInput,
    repeat: Repeat,
    attached: bool,
    held: [u8; MAX_HELD],
}

impl<R: TransferRing> KeyboardService<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            keyboard: BootKeyboard::new(ring),
            console: ConsoleInput::new(),
            repeat: Repeat::new(RepeatSettings::DEFAULT),
            attached: false,
            held: [0; MAX_HELD],
        }
    }

    /// Wrap a transport with a selected keymap layout.
    pub const fn with_layout(ring: R, layout: Layout) -> Self {
        Self {
            keyboard: BootKeyboard::with_layout(ring, layout),
            console: ConsoleInput::new(),
            repeat: Repeat::new(RepeatSettings::DEFAULT),
            attached: false,
            held: [0; MAX_HELD],
        }
    }

    /// Select boot protocol and validate the report descriptor.
    ///
    /// This is also the re-enumeration entry point after a detach: the keyboard state, locks and
    /// repeat are reset and the descriptor is validated again. Call [`Self::detach`] first if the
    /// stuck-key release events are needed.
    ///
    /// # Errors
    ///
    /// The descriptor/transport errors from [`BootKeyboard::init`].
    pub fn init(&mut self, report_descriptor: &[u8]) -> Result<(), Error> {
        self.attached = false;
        self.reset_state();
        self.keyboard.init(report_descriptor)?;
        self.attached = true;
        Ok(())
    }

    /// Mark the device absent and release every held key and modifier.
    ///
    /// The returned events are synthetic releases for the usages still held at detach; they are
    /// not queued as console bytes. The repeat is stopped, the keyboard modifiers/keys/locks are
    /// reset, and [`Self::poll_at`] returns [`Error::NotInitialized`] until a new [`Self::init`].
    /// Detaching twice is harmless and returns no events.
    pub fn detach(&mut self) -> KeyEvents {
        self.attached = false;
        self.repeat.clear();
        self.reset_keyboard();
        self.release_stuck()
    }

    /// True while a device is attached and initialized.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
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
        if !self.attached {
            return Err(Error::NotInitialized);
        }
        let Some(events) = self.keyboard.poll()? else {
            return Ok(0);
        };
        let mut queued = 0usize;
        for event in events.iter() {
            self.track(event);
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
        if !self.attached {
            return 0;
        }
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

    /// The mutable transport, for hotplug and re-enumeration.
    pub const fn ring_mut(&mut self) -> &mut R {
        self.keyboard.ring_mut()
    }

    /// The keyboard model.
    #[must_use]
    pub const fn keyboard(&self) -> &BootKeyboard<R> {
        &self.keyboard
    }

    /// Reset the model state and repeat without emitting releases.
    fn reset_state(&mut self) {
        self.repeat.clear();
        self.reset_keyboard();
        self.held = [0; MAX_HELD];
    }

    /// Reset modifiers, held keys, Caps Lock and Num Lock while preserving the layout.
    fn reset_keyboard(&mut self) {
        let layout = self.keyboard.keyboard().layout();
        *self.keyboard.keyboard_mut() = Keyboard::with_layout(layout);
    }

    /// Track one key transition for the detach release batch.
    fn track(&mut self, event: &KeyEvent) {
        if event.pressed {
            if !self.held.contains(&event.usage)
                && let Some(slot) = self.held.iter_mut().find(|usage| **usage == 0)
            {
                *slot = event.usage;
            }
        } else if let Some(slot) = self.held.iter_mut().find(|usage| **usage == event.usage) {
            *slot = 0;
        }
    }

    /// Build synthetic releases for every held usage and clear the held set.
    fn release_stuck(&mut self) -> KeyEvents {
        let mut releases = KeyEvents::default();
        for usage in self.held {
            if usage == 0 {
                continue;
            }
            if let Some(slot) = releases.events.get_mut(releases.count) {
                *slot = KeyEvent {
                    usage,
                    pressed: false,
                    ascii: None,
                };
                releases.count = releases.count.saturating_add(1);
            }
        }
        self.held = [0; MAX_HELD];
        releases
    }
}

#[cfg(test)]
#[path = "service_ut.rs"]
mod tests;
