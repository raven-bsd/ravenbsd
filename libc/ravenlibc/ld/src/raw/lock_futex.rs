// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The futex-backed lock primitive (unused before threads exist).

use crate::lock::LockState;

/// A futex lock word.
pub struct FutexLock {
    state: LockState,
}

impl FutexLock {
    /// A new lock.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: LockState::new(),
        }
    }

    /// Try to take the lock.
    pub fn try_lock(&mut self, owner: u32) -> bool {
        self.state.try_acquire(owner)
    }

    /// Release the lock.
    pub fn unlock(&mut self, owner: u32) -> bool {
        self.state.release(owner)
    }
}

impl Default for FutexLock {
    fn default() -> Self {
        Self::new()
    }
}
