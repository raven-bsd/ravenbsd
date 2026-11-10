// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Futex-based `Mutex` and `Once` for the RavenBSD user runtime.
//!
//! The mutex is not fair and has no poisoning: a panicking holder on a target without unwinding
//! aborts the process, so there is no recoverable poisoned state to model. `Once` runs exactly one
//! closure even when several threads race.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// Wait while `word` equals `expected`.
fn wait(word: &AtomicU32, expected: u32) {
    #[cfg(not(test))]
    {
        let _ = crate::sys::futex_wait(word, expected, None);
    }
    #[cfg(test)]
    {
        let _ = (word, expected);
        std::thread::yield_now();
    }
}

/// Wake at most `count` waiters.
fn wake(word: &AtomicU32, count: u32) {
    #[cfg(not(test))]
    {
        let _ = crate::sys::futex_wake(word, count);
    }
    #[cfg(test)]
    {
        let _ = (word, count);
    }
}

/// A futex-based mutual-exclusion lock.
pub struct Mutex<T> {
    state: AtomicU32,
    value: UnsafeCell<T>,
}

// SAFETY: the atomic state serializes access to `value`; moving the mutex between threads is safe
// when the protected value is `Send`.
unsafe impl<T: Send> Send for Mutex<T> {}
// SAFETY: `lock` grants exclusive access through the guard and `Drop` releases it.
unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    /// Create a new unlocked mutex.
    pub const fn new(value: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquire the mutex, waiting on the futex while another thread holds it.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            if self
                .state
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return MutexGuard { mutex: self };
            }
            wait(&self.state, 1);
        }
    }

    /// Try to acquire the mutex without waiting.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| MutexGuard { mutex: self })
    }
}

/// The RAII guard returned by [`Mutex::lock`].
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the mutex exclusively until it is dropped.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the mutex exclusively until it is dropped.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.state.store(0, Ordering::Release);
        wake(&self.mutex.state, 1);
    }
}

/// A one-time initialisation cell.
pub struct Once {
    state: AtomicU32,
}

impl Once {
    /// Create an uninitialised `Once`.
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
        }
    }

    /// Run `closure` exactly once across all threads.
    pub fn call_once<F: FnOnce()>(&self, closure: F) {
        if self.state.load(Ordering::Acquire) == 2 {
            return;
        }
        match self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => {
                closure();
                self.state.store(2, Ordering::Release);
                wake(&self.state, u32::MAX);
            }
            Err(1) => {
                while self.state.load(Ordering::Acquire) != 2 {
                    wait(&self.state, 1);
                }
            }
            Err(_) => {}
        }
    }

    /// True when the closure has completed.
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == 2
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU32;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn mutex_serializes_increments() {
        let counter = Arc::new(Mutex::new(0u32));
        let mut threads = Vec::new();
        for _ in 0..4 {
            let counter = Arc::clone(&counter);
            threads.push(thread::spawn(move || {
                for _ in 0..1_000 {
                    *counter.lock() += 1;
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(*counter.lock(), 4_000);
    }

    #[test]
    fn try_lock_fails_while_held() {
        let mutex = Mutex::new(1u8);
        let guard = mutex.lock();
        assert!(mutex.try_lock().is_none());
        drop(guard);
        assert!(mutex.try_lock().is_some());
    }

    #[test]
    fn once_runs_the_closure_once_under_contention() {
        let once = Arc::new(Once::new());
        let count = Arc::new(AtomicU32::new(0));
        let mut threads = Vec::new();
        for _ in 0..8 {
            let once = Arc::clone(&once);
            let count = Arc::clone(&count);
            threads.push(thread::spawn(move || {
                once.call_once(|| {
                    count.fetch_add(1, Ordering::SeqCst);
                });
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(once.is_completed());
    }

    #[test]
    fn once_skips_later_calls() {
        let once = Once::new();
        let mut count = 0;
        once.call_once(|| count += 1);
        once.call_once(|| count += 1);
        assert_eq!(count, 1);
    }
}
