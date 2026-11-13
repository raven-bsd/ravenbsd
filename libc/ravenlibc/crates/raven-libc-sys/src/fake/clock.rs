// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Virtual clock of the fake kernel.
//!
//! Mirrors `kernel/src/syscall/basic.rs:69-93` (`CLOCK_GET`) and `basic.rs:31-34`
//! (`NANOSLEEP` completes and returns 0).

use raven_abi::errno::Errno;
use raven_abi::types::{Timespec, clock};

/// A deterministic virtual clock.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Monotonic nanoseconds since the fake boot.
    pub monotonic_ns: u64,
    /// Realtime nanoseconds since the Unix epoch at fake boot.
    pub realtime_base_ns: u64,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock {
    /// Start at one millisecond of monotonic time and a fixed realtime epoch.
    pub const fn new() -> Self {
        Self {
            monotonic_ns: 1_000_000,
            realtime_base_ns: 1_700_000_000 * 1_000_000_000,
        }
    }

    /// Read a supported clock.
    pub fn get(&self, clock_id: u32) -> Result<Timespec, Errno> {
        let ns = match clock_id {
            clock::MONOTONIC => self.monotonic_ns,
            clock::REALTIME => self.realtime_base_ns + self.monotonic_ns,
            _ => return Err(Errno::EINVAL),
        };
        Ok(Timespec {
            tv_sec: (ns / 1_000_000_000) as i64,
            tv_nsec: (ns % 1_000_000_000) as i64,
        })
    }

    /// Sleep advances the virtual clock exactly.
    pub fn sleep(&mut self, ns: u64) {
        self.monotonic_ns = self.monotonic_ns.saturating_add(ns);
    }

    /// Advance by `ns` from a test.
    pub fn advance(&mut self, ns: u64) {
        self.sleep(ns);
    }
}
