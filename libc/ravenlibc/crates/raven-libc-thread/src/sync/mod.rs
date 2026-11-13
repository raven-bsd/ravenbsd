// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Internal synchronisation: the congestion lock, the futex wait, the timed wait, the creation
//! inhibit lock and the synchronised call.

pub mod lock;
pub mod ptc;
pub mod synccall;
pub mod timedwait;
pub mod wait;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;

/// The lock-order ranks of section 4.7 (outermost first).
pub mod rank {
    /// The atfork handler-list lock.
    pub const ATFORK: u8 = 1;
    /// The dynamic-loader lock.
    pub const LOADER: u8 = 2;
    /// The pthread key rwlock.
    pub const KEYS: u8 = 3;
    /// The thread-creation inhibit lock.
    pub const PTC: u8 = 4;
    /// The group of subsystem list locks (array order).
    pub const SUBSYSTEM: u8 = 5;
    /// A per-FILE lock (ascending address).
    pub const FILE: u8 = 6;
    /// The allocator locks.
    pub const MALLOC: u8 = 7;
    /// The per-thread kill lock.
    pub const KILL: u8 = 8;
    /// The thread-list lock.
    pub const THREAD_LIST: u8 = 9;
}

/// A debug rank checker: records the highest rank held by each thread and reports an inversion.
///
/// The shipped lock code does not call it on the hot path; the assertion is part of the debug
/// build and is exercised by the lock-order test with a deliberately inverted pair.
pub struct RankGuard {
    held: [u8; 16],
    depth: usize,
}

impl RankGuard {
    /// An empty guard.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            held: [0; 16],
            depth: 0,
        }
    }

    /// Record taking a lock of `rank`; returns the rank held before, or `None` on inversion.
    pub fn push(&mut self, rank: u8) -> Option<u8> {
        let previous = self.current();
        if let Some(held) = previous
            && rank < held
        {
            return None;
        }
        if let Some(slot) = self.held.get_mut(self.depth) {
            *slot = rank;
            self.depth = self.depth.wrapping_add(1);
        }
        previous
    }

    /// Record releasing the innermost lock.
    pub fn pop(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// The innermost rank currently held.
    #[must_use]
    pub fn current(&self) -> Option<u8> {
        if self.depth == 0 {
            None
        } else {
            self.held.get(self.depth - 1).copied()
        }
    }
}

impl Default for RankGuard {
    fn default() -> Self {
        Self::new()
    }
}
