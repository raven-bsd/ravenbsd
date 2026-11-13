// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process-wide lock seam.
//!
//! `raven-libc-platform` is a library, not a program: the shell installs the lock that serialises
//! the fd table and the OFD arena across threads (a futex mutex, the `userland/rt/src/sync.rs`
//! model). Tests use [`NoLock`] because every test runs single-threaded; the concurrency tests
//! wrap an explicit host mutex around the table instead.
//!
//! The lock is held only for index and refcount work, never across a syscall (doc 11 section
//! 3.5). [`LockGuard`] releases it on every return path, including early error returns.

pub mod lockf;

pub use lockf::{RecordLock, lockf};

/// A mutual-exclusion primitive supplied by the shell.
pub trait Lock {
    /// Acquire the lock.
    fn acquire(&self);
    /// Release the lock.
    fn release(&self);
}

/// Single-threaded no-op lock.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLock;

impl Lock for NoLock {
    #[inline]
    fn acquire(&self) {}
    #[inline]
    fn release(&self) {}
}

/// Scoped acquisition of a [`Lock`]; release happens in `Drop`.
pub struct LockGuard<'a, L: Lock + ?Sized> {
    lock: &'a L,
}

impl<L: Lock + ?Sized> Drop for LockGuard<'_, L> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

impl<L: Lock + ?Sized> LockGuard<'_, L> {
    /// The guarded lock.
    pub fn lock(&self) -> &L {
        self.lock
    }
}

/// Acquire `lock` and return a guard that releases it on drop.
pub fn guard<L: Lock + ?Sized>(lock: &L) -> LockGuard<'_, L> {
    lock.acquire();
    LockGuard { lock }
}

#[cfg(test)]
#[path = "lock_ut.rs"]
mod tests;
