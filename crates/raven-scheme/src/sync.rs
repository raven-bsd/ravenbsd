// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A minimal spin lock used for the per-server deferred-reply table.
//!
//! This is not a fairness primitive and is never used on a contended hot path; it only protects
//! the small table shared by a server and its `Completer` clones.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A spin lock around `T`.
pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    /// Create a new unlocked spin lock.
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock, spinning until it becomes available.
    pub fn lock(&self) -> SpinGuard<'_, T> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinGuard { lock: self }
    }
}

// SAFETY: the atomic flag serializes every access to the `UnsafeCell`; `T: Send` is required so
// moving a value protected by the lock between threads is sound.
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// The RAII guard returned by [`SpinLock::lock`].
pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the lock exclusively until it is dropped.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the lock exclusively until it is dropped.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use std::thread;

    #[test]
    fn lock_serializes_a_counter() {
        let lock = Arc::new(SpinLock::new(0u64));
        let mut threads = Vec::new();
        for _ in 0..4 {
            let lock = Arc::clone(&lock);
            threads.push(thread::spawn(move || {
                for _ in 0..1_000 {
                    *lock.lock() += 1;
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(*lock.lock(), 4_000);
    }

    #[test]
    fn guard_releases_on_drop() {
        let lock = SpinLock::new(1u8);
        {
            let mut guard = lock.lock();
            *guard = 2;
            assert_eq!(*guard, 2);
        }
        assert_eq!(*lock.lock(), 2);
    }
}
