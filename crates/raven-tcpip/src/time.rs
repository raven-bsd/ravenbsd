// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Monotonic time supplied by the caller (contract C15).

use core::fmt;

/// A monotonic instant in milliseconds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Instant(i64);

impl Instant {
    /// The zero instant.
    pub const ZERO: Self = Self(0);

    /// Construct an instant from milliseconds.
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Return the instant in milliseconds.
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// Add a duration, returning `None` on overflow.
    #[must_use]
    pub const fn checked_add(self, duration: Duration) -> Option<Self> {
        match self.0.checked_add(duration.as_millis_i64()) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Add a duration, saturating at the representable maximum.
    #[must_use]
    pub const fn saturating_add(self, duration: Duration) -> Self {
        Self(self.0.saturating_add(duration.as_millis_i64()))
    }

    /// The duration since an earlier instant, saturating at zero.
    #[must_use]
    pub const fn saturating_since(self, earlier: Self) -> Duration {
        let delta = self.0.saturating_sub(earlier.0);
        if delta <= 0 {
            Duration::ZERO
        } else {
            Duration::from_millis(delta as u64)
        }
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instant({} ms)", self.0)
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ms", self.0)
    }
}

/// A non-negative duration in milliseconds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration(u64);

impl Duration {
    /// A zero duration.
    pub const ZERO: Self = Self(0);

    /// Construct a duration from milliseconds.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Return the duration in milliseconds.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Whether this is a zero duration.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    const fn as_millis_i64(self) -> i64 {
        if self.0 > i64::MAX as u64 {
            i64::MAX
        } else {
            self.0 as i64
        }
    }
}

impl fmt::Debug for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Duration({} ms)", self.0)
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ms", self.0)
    }
}

#[cfg(test)]
#[path = "time_ut.rs"]
mod tests;
