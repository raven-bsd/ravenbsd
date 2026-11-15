// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RP1 clock tree as data and checked transitions.
//!
//! The device tree gives the clock provider window, `#clock-cells = 1` and the rates the firmware
//! assigned to a handful of clock IDs. It does not name the clocks or give their register
//! encoding, so this module models IDs, rates, parent links, enable state and the dependency
//! invariants. The clock IDs and the assigned rates are OBSERVED in the Pi 5 DTB; the real limits
//! and register layout are UNVERIFIED.

use core::fmt;

/// Largest number of clock IDs the model holds.
pub const MAX_CLOCKS: usize = 32;

/// One clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSpec {
    /// Clock ID as used by `#clock-cells = 1`.
    pub id: u8,
    /// Current rate in Hz.
    pub rate_hz: u32,
    /// Highest rate the caller allows. The real limit is UNVERIFIED.
    pub max_rate_hz: u32,
    /// Parent clock, when the caller knows it. The device tree does not supply it.
    pub parent: Option<u8>,
    /// Gate state.
    pub enabled: bool,
}

impl ClockSpec {
    /// A clock that is on at `rate_hz`.
    #[must_use]
    pub const fn new(id: u8, rate_hz: u32, max_rate_hz: u32) -> Self {
        Self {
            id,
            rate_hz,
            max_rate_hz,
            parent: None,
            enabled: true,
        }
    }

    /// Attach a parent clock.
    #[must_use]
    pub const fn with_parent(self, parent: u8) -> Self {
        Self {
            parent: Some(parent),
            ..self
        }
    }
}

/// Why a clock transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    /// The clock ID is not in the tree.
    UnknownClock(u8),
    /// The requested rate is zero.
    RateZero,
    /// The requested rate exceeds the clock's limit.
    RateTooHigh {
        /// Clock ID.
        id: u8,
        /// Requested rate.
        rate: u32,
        /// Limit.
        max: u32,
    },
    /// The parent clock is not in the tree.
    UnknownParent(u8),
    /// The child would run faster than its parent.
    ParentTooSlow {
        /// Child ID.
        id: u8,
        /// Parent ID.
        parent: u8,
        /// Requested child rate.
        rate: u32,
    },
    /// The parent gate is off.
    ParentDisabled(u8),
    /// A child gate is on, so the parent cannot be disabled.
    ChildEnabled(u8),
    /// The tree is full or the ID is already present.
    Duplicate(u8),
    /// The tree has no free slot.
    Full,
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownClock(id) => write!(f, "clock {id} is not in the tree"),
            Self::RateZero => f.write_str("clock rate must be non-zero"),
            Self::RateTooHigh { id, rate, max } => {
                write!(f, "clock {id} rate {rate} exceeds {max}")
            }
            Self::UnknownParent(parent) => write!(f, "parent clock {parent} is not in the tree"),
            Self::ParentTooSlow { id, parent, rate } => {
                write!(f, "clock {id} rate {rate} exceeds its parent {parent}")
            }
            Self::ParentDisabled(parent) => write!(f, "parent clock {parent} is disabled"),
            Self::ChildEnabled(child) => write!(f, "clock {child} is still enabled"),
            Self::Duplicate(id) => write!(f, "clock {id} is already present"),
            Self::Full => f.write_str("clock tree is full"),
        }
    }
}

impl core::error::Error for ClockError {}

/// A fixed-capacity clock tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockTree {
    clocks: [Option<ClockSpec>; MAX_CLOCKS],
    count: usize,
}

impl ClockTree {
    /// An empty tree.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            clocks: [None; MAX_CLOCKS],
            count: 0,
        }
    }

    /// Add a clock.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::Duplicate`] for a repeated ID or a full tree.
    pub fn insert(&mut self, spec: ClockSpec) -> Result<(), ClockError> {
        if self.get(spec.id).is_some() {
            return Err(ClockError::Duplicate(spec.id));
        }
        match self.clocks.get_mut(self.count) {
            Some(slot) => {
                *slot = Some(spec);
                self.count = self.count.saturating_add(1);
                Ok(())
            }
            None => Err(ClockError::Full),
        }
    }

    /// Number of clocks.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// True when the tree is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterate the clocks in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &ClockSpec> {
        self.clocks.iter().flatten()
    }

    /// Look up a clock.
    #[must_use]
    pub fn get(&self, id: u8) -> Option<&ClockSpec> {
        self.iter().find(|clock| clock.id == id)
    }

    /// Change a clock rate, returning the previous rate.
    ///
    /// A child never runs faster than its parent. The parent itself is not reprogrammed.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError`] for an unknown clock or parent, a zero rate, a rate above the
    /// clock's limit, or a rate above the parent's current rate.
    pub fn set_rate(&mut self, id: u8, rate_hz: u32) -> Result<u32, ClockError> {
        let spec = *self.get(id).ok_or(ClockError::UnknownClock(id))?;
        if rate_hz == 0 {
            return Err(ClockError::RateZero);
        }
        if rate_hz > spec.max_rate_hz {
            return Err(ClockError::RateTooHigh {
                id,
                rate: rate_hz,
                max: spec.max_rate_hz,
            });
        }
        if let Some(parent) = spec.parent {
            let parent_spec = *self.get(parent).ok_or(ClockError::UnknownParent(parent))?;
            if rate_hz > parent_spec.rate_hz {
                return Err(ClockError::ParentTooSlow {
                    id,
                    parent,
                    rate: rate_hz,
                });
            }
        }
        if let Some(slot) = self
            .clocks
            .iter_mut()
            .take(self.count)
            .flatten()
            .find(|clock| clock.id == id)
        {
            slot.rate_hz = rate_hz;
        }
        Ok(spec.rate_hz)
    }

    /// Enable a clock; its parent must be enabled first.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::UnknownClock`] or [`ClockError::ParentDisabled`].
    pub fn enable(&mut self, id: u8) -> Result<(), ClockError> {
        let spec = *self.get(id).ok_or(ClockError::UnknownClock(id))?;
        if let Some(parent) = spec.parent {
            let parent_spec = *self.get(parent).ok_or(ClockError::UnknownParent(parent))?;
            if !parent_spec.enabled {
                return Err(ClockError::ParentDisabled(parent));
            }
        }
        self.set_enabled(id, true);
        Ok(())
    }

    /// Disable a clock; no enabled child may depend on it.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::UnknownClock`] or [`ClockError::ChildEnabled`].
    pub fn disable(&mut self, id: u8) -> Result<(), ClockError> {
        if self.get(id).is_none() {
            return Err(ClockError::UnknownClock(id));
        }
        if let Some(child) = self
            .iter()
            .find(|clock| clock.parent == Some(id) && clock.enabled)
        {
            return Err(ClockError::ChildEnabled(child.id));
        }
        self.set_enabled(id, false);
        Ok(())
    }

    fn set_enabled(&mut self, id: u8, enabled: bool) {
        if let Some(slot) = self
            .clocks
            .iter_mut()
            .take(self.count)
            .flatten()
            .find(|clock| clock.id == id)
        {
            slot.enabled = enabled;
        }
    }
}

impl Default for ClockTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "clocks_ut.rs"]
mod tests;
