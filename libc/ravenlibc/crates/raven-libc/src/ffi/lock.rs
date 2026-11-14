// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The shell's one short spin lock, used by the process-wide scratch areas of [`super::tcbbuf`].
//! No lock of the shell is ever held across a backend call; the C64 network lock (C63's
//! `LockedNet`) is deferred with that family. Until C57 installs the thread support every libc
//! call is single-threaded, so the lock only serialises a re-entrant call from a host test.

use core::sync::atomic::{AtomicBool, Ordering};

/// A fair-enough spin lock over one atomic word.
#[derive(Default)]
pub(crate) struct SpinLock {
    held: AtomicBool,
}

impl SpinLock {
    /// An unlocked lock.
    pub(crate) const fn new() -> Self {
        Self {
            held: AtomicBool::new(false),
        }
    }

    /// Take the lock.
    pub(crate) fn acquire(&self) {
        while self.held.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
    }

    /// Release the lock.
    pub(crate) fn release(&self) {
        self.held.store(false, Ordering::Release);
    }

    /// Whether the lock is currently held (tests).
    #[cfg(test)]
    pub(crate) fn is_held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "lock_ut.rs"]
mod tests;
