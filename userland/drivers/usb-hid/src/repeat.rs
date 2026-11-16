// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Auto-repeat model for the keyboard service.

/// Delay before the first repeat and the interval between repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepeatSettings {
    /// Time from the initial press to the first repeat.
    pub delay_ns: u64,
    /// Interval between repeats; zero disables repetition.
    pub period_ns: u64,
}

impl RepeatSettings {
    /// The default 500 ms delay and 50 ms (20/s) period.
    pub const DEFAULT: Self = Self {
        delay_ns: 500_000_000,
        period_ns: 50_000_000,
    };

    /// A settings value from milliseconds.
    #[must_use]
    pub const fn from_millis(delay_ms: u16, period_ms: u16) -> Self {
        Self {
            delay_ns: (delay_ms as u64).saturating_mul(1_000_000),
            period_ns: (period_ms as u64).saturating_mul(1_000_000),
        }
    }
}

impl Default for RepeatSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The last ASCII key held and its next repeat deadline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Repeat {
    settings: RepeatSettings,
    usage: u8,
    ascii: Option<u8>,
    next_ns: u64,
    active: bool,
}

impl Repeat {
    /// A repeater with the given settings.
    #[must_use]
    pub const fn new(settings: RepeatSettings) -> Self {
        Self {
            settings,
            usage: 0,
            ascii: None,
            next_ns: 0,
            active: false,
        }
    }

    /// The current settings.
    #[must_use]
    pub const fn settings(&self) -> RepeatSettings {
        self.settings
    }

    /// Replace the settings.
    pub fn set_settings(&mut self, settings: RepeatSettings) {
        self.settings = settings;
    }

    /// A key press: an ASCII key becomes the repeating key, a non-ASCII press leaves the current
    /// repeat alone (a modifier must not cancel a held letter).
    pub fn press(&mut self, usage: u8, ascii: Option<u8>, now_ns: u64) {
        if let Some(ascii) = ascii {
            self.usage = usage;
            self.ascii = Some(ascii);
            self.active = true;
            self.next_ns = now_ns.saturating_add(self.settings.delay_ns);
        }
    }

    /// A key release; releasing the repeating key stops it.
    pub fn release(&mut self, usage: u8) {
        if self.active && usage == self.usage {
            self.clear();
        }
    }

    /// Produce one repeat byte when the deadline has passed.
    pub fn tick(&mut self, now_ns: u64) -> Option<u8> {
        if !self.active || self.settings.period_ns == 0 {
            return None;
        }
        if now_ns < self.next_ns {
            return None;
        }
        self.next_ns = now_ns.saturating_add(self.settings.period_ns);
        self.ascii
    }

    /// Stop repeating.
    pub fn clear(&mut self) {
        self.usage = 0;
        self.ascii = None;
        self.next_ns = 0;
        self.active = false;
    }

    /// True while a key is repeating.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
#[path = "repeat_ut.rs"]
mod tests;
