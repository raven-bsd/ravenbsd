// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-protocol keyboard reports.

use crate::keymap::{CAPS_LOCK, Layout, MODIFIER_FIRST, NUM_LOCK, key_to_ascii_layout};

/// Boot-protocol keyboard report length.
pub const BOOT_REPORT_LEN: usize = 8;
/// Maximum key events one report can emit (6 releases, 6 presses, 8 modifier changes).
pub const MAX_EVENTS: usize = 24;

/// One eight-byte boot keyboard report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BootReport {
    /// Modifier bitmap: LCtrl, LShift, LAlt, LGui, RCtrl, RShift, RAlt, RGui.
    pub modifiers: u8,
    /// Six key usage identifiers; zero means an empty slot.
    pub keys: [u8; 6],
}

impl BootReport {
    /// Decode an eight-byte report.
    ///
    /// # Errors
    ///
    /// [`crate::Error::ShortReport`] when `data` is shorter than eight bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::Error> {
        let bytes = data
            .get(..BOOT_REPORT_LEN)
            .ok_or(crate::Error::ShortReport)?;
        let modifiers = bytes.first().copied().unwrap_or(0);
        let mut keys = [0u8; 6];
        if let (Some(target), Some(source)) = (keys.get_mut(..), bytes.get(2..BOOT_REPORT_LEN)) {
            target.copy_from_slice(source);
        }
        Ok(Self { modifiers, keys })
    }

    /// True for the boot-protocol ErrorRollOver report (all six slots `0x01`).
    #[must_use]
    pub fn is_rollover(&self) -> bool {
        self.keys.iter().all(|key| *key == 0x01)
    }

    /// True when at least one modifier bit is set.
    #[must_use]
    pub fn shift(&self) -> bool {
        self.modifiers & 0x22 != 0
    }
}

/// One decoded key transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyEvent {
    /// HID usage identifier.
    pub usage: u8,
    /// True on a press, false on a release.
    pub pressed: bool,
    /// ASCII translation under the current shift/caps state, when the key has one.
    pub ascii: Option<u8>,
}

/// A bounded batch of key events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyEvents {
    /// Event storage.
    pub events: [KeyEvent; MAX_EVENTS],
    /// Number of valid entries.
    pub count: usize,
}

impl KeyEvents {
    /// Visit the valid events.
    pub fn iter(&self) -> impl Iterator<Item = &KeyEvent> {
        self.events.iter().take(self.count)
    }

    fn push(&mut self, event: KeyEvent) {
        if let Some(slot) = self.events.get_mut(self.count) {
            *slot = event;
            self.count = self.count.saturating_add(1);
        }
    }
}

/// Keyboard state: previous report, modifiers, locks and layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keyboard {
    modifiers: u8,
    keys: [u8; 6],
    caps: bool,
    num_lock: bool,
    layout: Layout,
}

impl Keyboard {
    /// A fresh US keyboard with no keys held.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            modifiers: 0,
            keys: [0; 6],
            caps: false,
            num_lock: false,
            layout: Layout::Us,
        }
    }

    /// A fresh keyboard with the selected layout.
    #[must_use]
    pub const fn with_layout(layout: Layout) -> Self {
        Self {
            modifiers: 0,
            keys: [0; 6],
            caps: false,
            num_lock: false,
            layout,
        }
    }

    /// The active layout.
    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.layout
    }

    /// Select the layout for subsequent reports.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// The current modifier bitmap.
    #[must_use]
    pub const fn modifiers(&self) -> u8 {
        self.modifiers
    }

    /// True when Caps Lock is active.
    #[must_use]
    pub const fn caps(&self) -> bool {
        self.caps
    }

    /// True when Num Lock is active.
    #[must_use]
    pub const fn num_lock(&self) -> bool {
        self.num_lock
    }

    /// True while either Shift bit is held.
    #[must_use]
    pub const fn shift(&self) -> bool {
        crate::keymap::shift(self.modifiers)
    }

    /// True while either Control bit is held.
    #[must_use]
    pub const fn control(&self) -> bool {
        crate::keymap::control(self.modifiers)
    }

    /// True while Left Alt is held.
    #[must_use]
    pub const fn alt(&self) -> bool {
        crate::keymap::alt(self.modifiers)
    }

    /// True while Right Alt (AltGr) is held.
    #[must_use]
    pub const fn altgr(&self) -> bool {
        crate::keymap::altgr(self.modifiers)
    }

    /// True while either GUI bit is held.
    #[must_use]
    pub const fn gui(&self) -> bool {
        crate::keymap::gui(self.modifiers)
    }

    /// Decode one report and return the key transitions.
    ///
    /// An ErrorRollOver report is ignored without changing the held-key state. Caps Lock and Num
    /// Lock toggle on their press before the ASCII of every other key in the same report is
    /// computed.
    pub fn update(&mut self, report: &BootReport) -> KeyEvents {
        let mut events = KeyEvents::default();
        if report.is_rollover() {
            return events;
        }
        if report.keys.contains(&CAPS_LOCK) && !self.keys.contains(&CAPS_LOCK) {
            self.caps = !self.caps;
        }
        if report.keys.contains(&NUM_LOCK) && !self.keys.contains(&NUM_LOCK) {
            self.num_lock = !self.num_lock;
        }
        let changed = self.modifiers ^ report.modifiers;
        let mut bit = 0u8;
        while bit < 8 {
            if changed & (1 << bit) != 0 {
                events.push(KeyEvent {
                    usage: MODIFIER_FIRST.saturating_add(bit),
                    pressed: report.modifiers & (1 << bit) != 0,
                    ascii: None,
                });
            }
            bit = bit.saturating_add(1);
        }
        self.modifiers = report.modifiers;

        for key in self.keys {
            if key != 0 && !report.keys.contains(&key) {
                events.push(KeyEvent {
                    usage: key,
                    pressed: false,
                    ascii: translate(key, self.modifiers, self.caps, self.num_lock, self.layout),
                });
            }
        }
        for key in report.keys {
            if key != 0 && !self.keys.contains(&key) {
                events.push(KeyEvent {
                    usage: key,
                    pressed: true,
                    ascii: translate(key, self.modifiers, self.caps, self.num_lock, self.layout),
                });
            }
        }
        self.keys = report.keys;
        events
    }
}

/// ASCII for one usage under the current layout, modifiers and locks.
fn translate(usage: u8, modifiers: u8, caps: bool, num_lock: bool, layout: Layout) -> Option<u8> {
    key_to_ascii_layout(usage, layout, modifiers, caps, num_lock)
}

#[cfg(test)]
#[path = "report_ut.rs"]
mod tests;
