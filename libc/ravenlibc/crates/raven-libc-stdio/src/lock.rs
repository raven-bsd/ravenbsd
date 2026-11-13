// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/__lockfile.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor file has no per-file header; it is covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
//! The per-FILE recursive owner-tid lock and the [`FileGuard`] used by every operation.
//!
//! `lockfile` transcribes `musl:src/stdio/__lockfile.c`: the low 30 bits of the word hold the
//! owner tid, `MAYBE_WAITERS` marks a waiter, a recursive acquisition returns `false` so the
//! inner guard does not release the outer hold, and the CAS ladder is the donor's. `unlockfile`
//! is `swap(0)` plus one wake when the waiter bit was set.
//!
//! The donor's `lock = -1` single-thread shortcut is removed (A-stdio-9): `lock` starts at 0.
//! The single-thread fast path lives in [`FileGuard`] and applies only to the implicit guards;
//! `flockfile`/`ftrylockfile` always take the real lock so a later `pthread_create` sees an
//! owned stream. `lockcount` keeps the explicit nesting depth; the per-thread locked-file list
//! of `__register_locked_file` is omitted (O7, documented in the README).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::file::{File, MAYBE_WAITERS};
use crate::traits::Env;

/// The raw lock word of a FILE.
pub(crate) fn lock_word<E: Env>(file: &File<E>) -> &AtomicI32 {
    &file.st().lock
}

/// Take the stream lock.
///
/// Returns `false` when the calling thread already owns it (recursive), `true` when this call
/// acquired it and must release it.
pub(crate) fn lockfile<E: Env>(file: &File<E>) -> bool {
    let env = file.env();
    let word = lock_word(file);
    let tid = env.tid();
    let owner = word.load(Ordering::Relaxed);
    if owner & !MAYBE_WAITERS == tid {
        return false;
    }
    if word
        .compare_exchange(0, tid, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        return true;
    }
    loop {
        match word.compare_exchange(0, tid | MAYBE_WAITERS, Ordering::Acquire, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(owner) => {
                let marked = word
                    .compare_exchange(
                        owner,
                        owner | MAYBE_WAITERS,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok();
                if owner & MAYBE_WAITERS != 0 || marked {
                    env.wait(word, owner | MAYBE_WAITERS);
                }
            }
        }
    }
}

/// Release the stream lock.
pub(crate) fn unlockfile<E: Env>(file: &File<E>) {
    let env = file.env();
    let word = lock_word(file);
    if word.swap(0, Ordering::Release) & MAYBE_WAITERS != 0 {
        env.wake(word, 1);
    }
}

/// `flockfile`: take the lock and record one explicit nesting level.
pub fn flockfile<E: Env>(file: &File<E>) {
    if ftrylockfile(file) == 0 {
        return;
    }
    let _ = lockfile(file);
    file.st_mut().lockcount = 1;
}

/// `ftrylockfile`: a non-blocking explicit acquisition.
///
/// Returns 0 on success and -1 when another thread owns the stream (or the recursion limit is
/// reached).
pub fn ftrylockfile<E: Env>(file: &File<E>) -> i32 {
    let env = file.env();
    let word = lock_word(file);
    let tid = env.tid();
    let owner = word.load(Ordering::Relaxed);
    if owner & !MAYBE_WAITERS == tid {
        if file.st().lockcount == i64::MAX {
            return -1;
        }
        file.st_mut().lockcount = file.st().lockcount.saturating_add(1);
        return 0;
    }
    if owner != 0
        || word
            .compare_exchange(0, tid, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
    {
        return -1;
    }
    file.st_mut().lockcount = 1;
    0
}

/// `funlockfile`: drop one explicit nesting level and release at zero.
pub fn funlockfile<E: Env>(file: &File<E>) {
    let depth = file.st().lockcount;
    if depth == 1 {
        file.st_mut().lockcount = 0;
        unlockfile(file);
    } else if depth > 1 {
        file.st_mut().lockcount = depth - 1;
    }
}

/// The implicit lock guard of one operation (`FLOCK`/`FUNLOCK`).
pub struct FileGuard<'a, E: Env> {
    file: &'a File<E>,
    took: bool,
}

impl<'a, E: Env> FileGuard<'a, E> {
    /// Enter the stream.
    ///
    /// Skips the lock when `F_BYCALLER` is set (the caller owns it) and takes the single-thread
    /// fast path while `Lock::threaded()` is false. `took` records what actually happened, so a
    /// guard created in fast mode never releases a lock it did not take (trap S5).
    pub fn new(file: &'a File<E>) -> Self {
        if file.st().flags & crate::file::F_BYCALLER != 0 || !file.env().threaded() {
            return Self { file, took: false };
        }
        let took = lockfile(file);
        #[cfg(test)]
        if took {
            crate::fake_ut::order::file_acquired();
        }
        Self { file, took }
    }

    /// The guarded FILE.
    #[must_use]
    pub fn file(&self) -> &'a File<E> {
        self.file
    }
}

impl<E: Env> Drop for FileGuard<'_, E> {
    fn drop(&mut self) {
        if self.took {
            #[cfg(test)]
            crate::fake_ut::order::file_released();
            unlockfile(self.file);
        }
    }
}

#[cfg(test)]
#[path = "lock_ut.rs"]
mod tests;
