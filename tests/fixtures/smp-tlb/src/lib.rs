// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded-handshake helpers shared by the S76b fixture and its host tests.
//!
//! The fixture talks to a reader on an application processor through GO/DONE atomics. A lost
//! wake must fail in seconds with the handshake state instead of spending the qtest's 300 s case
//! budget, so the bound is wall-clock based and the report names GO, DONE, SEEN, EXITED and the
//! reader tid.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// Wall-clock bound for one GO/DONE handshake. A healthy TCG round is a few milliseconds; five
/// seconds catches a lost wake in seconds instead of spending the qtest's 300 s case budget.
pub const HANDSHAKE_TIMEOUT_NS: u64 = 5_000_000_000;

/// True once `now` is at or past the handshake bound that started at `start`.
pub fn deadline_reached(start: u64, now: u64) -> bool {
    now.saturating_sub(start) >= HANDSHAKE_TIMEOUT_NS
}

/// The handshake state printed on a timeout. The ABI exposes no current-CPU query, so the reader
/// tid is printed; the kernel's `smp-user: cpu N first user thread ... tid=` line maps it to a CPU.
pub struct StateSnapshot {
    /// Round the controller published.
    pub go: u32,
    /// Last round the reader completed.
    pub done: u32,
    /// Last value the reader observed.
    pub seen: u64,
    /// Reader exit marker (`0` running, `1` clean, `2` timeout, `3` clock failure).
    pub exited: u32,
    /// Reader thread id, stored when the reader starts.
    pub reader_tid: u32,
}

impl core::fmt::Display for StateSnapshot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "go={} done={} seen={:#x} exited={} reader_tid={}",
            self.go, self.done, self.seen, self.exited, self.reader_tid
        )
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
