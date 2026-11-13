// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Client-side blocking deadlines.

use crate::NetEnv;

/// A monotonic deadline; `None` means "wait forever".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadline {
    end: Option<u64>,
}

impl Deadline {
    /// Build a deadline from an optional millisecond timeout.
    #[must_use]
    pub fn new(env: &dyn NetEnv, timeout_ms: Option<u64>) -> Self {
        Self {
            end: timeout_ms.map(|timeout| env.monotonic_ms().saturating_add(timeout)),
        }
    }

    /// The remaining milliseconds, or `None` for an infinite deadline.
    #[must_use]
    pub fn remaining(&self, env: &dyn NetEnv) -> Option<u64> {
        self.end.map(|end| end.saturating_sub(env.monotonic_ms()))
    }

    /// Whether the deadline has passed.
    #[must_use]
    pub fn expired(&self, env: &dyn NetEnv) -> bool {
        self.end.is_some_and(|end| env.monotonic_ms() >= end)
    }

    /// The absolute end, for tests.
    #[must_use]
    pub const fn end(&self) -> Option<u64> {
        self.end
    }
}

#[cfg(test)]
#[path = "block_ut.rs"]
mod tests;
