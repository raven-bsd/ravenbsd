// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The cooperative fork-lock registry.

use alloc::vec::Vec;

use crate::errors::{EEXIST, Errno};

/// A lock that can be reset in a forked child.
pub trait ForkLock {
    /// Take the lock.
    fn lock(&self);
    /// Release the lock in the parent.
    fn unlock(&self);
    /// Reset to unlocked in the child, without running a normal unlock.
    fn reset(&self);
}

/// A registry of named locks in deterministic order.
#[derive(Clone, Default)]
pub struct LockRegistry {
    entries: Vec<(&'static str, &'static dyn ForkLock)>,
}

impl LockRegistry {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Register one lock; duplicate names are rejected.
    pub fn register(
        &mut self,
        name: &'static str,
        lock: &'static dyn ForkLock,
    ) -> Result<(), Errno> {
        if self.entries.iter().any(|(existing, _)| *existing == name) {
            return Err(EEXIST);
        }
        self.entries.push((name, lock));
        Ok(())
    }

    /// Take every lock in name order.
    pub fn lock_all(&mut self) {
        self.entries.sort_by_key(|(name, _)| *name);
        for (_, lock) in &self.entries {
            lock.lock();
        }
    }

    /// Release every lock in reverse name order.
    pub fn unlock_all(&mut self) {
        for (_, lock) in self.entries.iter().rev() {
            lock.unlock();
        }
    }

    /// Reset every lock in name order for a forked child.
    pub fn reset_all(&mut self) {
        for (_, lock) in &self.entries {
            lock.reset();
        }
    }

    /// Number of registered locks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no lock is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "lock_registry_ut.rs"]
mod tests;
