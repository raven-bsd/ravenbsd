// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigaction.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
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

//! The process disposition table and its lock-free reader (A-sol-19, rule S1).
//!
//! musl hands `sigaction` to the kernel. Raven's kernel only kicks (PG-7), so the table lives in
//! libc. Each of the 65 entries is a seqlock: `seq` is odd while a writer updates the triple and
//! even when it is stable. The entry path only reads, retrying while `seq` is odd or changed; it
//! never takes the writer lock. `sigaction` serialises writers with one futex word that the signal
//! path never touches.

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::errors::EINVAL_ERR;
use crate::sigset::SigSet;
use raven_libc_abi::errno::Errno;
#[cfg(target_os = "none")]
use raven_libc_sys::sys;

/// Number of table entries (`_NSIG`).
pub const ENTRY_COUNT: usize = 65;
/// Bits the entry path may spin before yielding the CPU.
pub const READ_SPINS: u32 = 1000;

/// `SA_NOCLDSTOP`.
pub const SA_NOCLDSTOP: u32 = 1;
/// `SA_NOCLDWAIT`.
pub const SA_NOCLDWAIT: u32 = 2;
/// `SA_SIGINFO`.
pub const SA_SIGINFO: u32 = 4;
/// `SA_RESTORER`: accepted but never stored (no restorer in Raven).
pub const SA_RESTORER: u32 = 0x0400_0000;
/// `SA_ONSTACK`.
pub const SA_ONSTACK: u32 = 0x0800_0000;
/// `SA_RESTART`.
pub const SA_RESTART: u32 = 0x1000_0000;
/// `SA_NODEFER`.
pub const SA_NODEFER: u32 = 0x4000_0000;
/// `SA_RESETHAND`.
pub const SA_RESETHAND: u32 = 0x8000_0000;

/// One stored disposition (handler 0 = `SIG_DFL`, 1 = `SIG_IGN`, else a function address).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Disposition {
    /// The handler word.
    pub handler: usize,
    /// The application `sa_mask` word (internal bits clear).
    pub mask: u64,
    /// The `sa_flags` word.
    pub flags: u32,
}

impl Disposition {
    /// `SIG_DFL`.
    pub const DEFAULT: Self = Self {
        handler: 0,
        mask: 0,
        flags: 0,
    };
    /// `SIG_IGN`.
    pub const IGNORE: Self = Self {
        handler: 1,
        mask: 0,
        flags: 0,
    };
    /// A function handler with no flags.
    #[must_use]
    pub const fn handler(address: usize) -> Self {
        Self {
            handler: address,
            mask: 0,
            flags: 0,
        }
    }
}

/// One seqlock entry.
struct Entry {
    seq: AtomicU32,
    handler: AtomicUsize,
    mask: AtomicU64,
    flags: AtomicU32,
}

impl Entry {
    const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            handler: AtomicUsize::new(0),
            mask: AtomicU64::new(0),
            flags: AtomicU32::new(0),
        }
    }
}

/// The 65-entry disposition table.
pub struct DispTable {
    entries: [Entry; ENTRY_COUNT],
    writer: AtomicU32,
    handler_set: AtomicU64,
}

impl DispTable {
    /// An all-default table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [const { Entry::new() }; ENTRY_COUNT],
            writer: AtomicU32::new(0),
            handler_set: AtomicU64::new(0),
        }
    }

    /// Read one disposition with the seqlock retry (lock-free).
    #[must_use]
    pub fn read(&self, sig: i32) -> Disposition {
        let Some(entry) = self.entry(sig) else {
            return Disposition::DEFAULT;
        };
        let mut spin = 0u32;
        loop {
            let first = entry.seq.load(Ordering::Acquire);
            if first & 1 == 0 {
                // LoadLoad ordering between the first sequence read, the field reads and the
                // confirming sequence read; `Acquire` alone only fences later accesses.
                core::sync::atomic::fence(Ordering::SeqCst);
                let handler = entry.handler.load(Ordering::Relaxed);
                let mask = entry.mask.load(Ordering::Relaxed);
                let flags = entry.flags.load(Ordering::Relaxed);
                core::sync::atomic::fence(Ordering::SeqCst);
                if entry.seq.load(Ordering::Acquire) == first {
                    return Disposition {
                        handler,
                        mask,
                        flags,
                    };
                }
            }
            spin = spin.wrapping_add(1);
            if spin.is_multiple_of(32) {
                relax();
            }
        }
    }

    /// Write one disposition (writers serialise through the futex word) and return the old one.
    pub fn write(&self, sig: i32, disposition: Disposition) -> Disposition {
        let old = self.read(sig);
        let Some(entry) = self.entry(sig) else {
            return old;
        };
        self.lock();
        entry.seq.fetch_add(1, Ordering::AcqRel);
        entry.handler.store(disposition.handler, Ordering::Relaxed);
        entry.mask.store(disposition.mask, Ordering::Relaxed);
        entry.flags.store(disposition.flags, Ordering::Relaxed);
        entry.seq.fetch_add(1, Ordering::Release);
        self.unlock();
        old
    }

    /// The set of signals with a function handler installed.
    #[must_use]
    pub fn handler_set(&self) -> SigSet {
        let mut set = SigSet::empty();
        if let Some(word) = set.0.first_mut() {
            *word = self.handler_set.load(Ordering::Acquire);
        }
        set
    }

    /// Record `sig` in the handler set (one atomic OR, as musl's `a_or_l`).
    pub fn note_handler(&self, sig: i32) {
        if (1..=64).contains(&sig) {
            let shift = (sig.wrapping_sub(1) & 63) as u32;
            self.handler_set
                .fetch_or(1u64.wrapping_shl(shift), Ordering::Release);
        }
    }

    fn entry(&self, sig: i32) -> Option<&Entry> {
        if !(1..=64).contains(&sig) {
            return None;
        }
        self.entries.get(sig.wrapping_sub(1) as usize)
    }

    fn lock(&self) {
        while self
            .writer
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            wait_word(&self.writer);
        }
    }

    fn unlock(&self) {
        self.writer.store(0, Ordering::Release);
        wake_word(&self.writer);
    }
}

impl Default for DispTable {
    fn default() -> Self {
        Self::new()
    }
}

/// `sigaction` bookkeeping: validate, update the table, report the old action.
pub fn sigaction(
    table: &DispTable,
    sig: i32,
    new: Option<&Disposition>,
    old: Option<&mut Disposition>,
) -> Result<(), Errno> {
    crate::action::sigaction(table, sig, new, old)
}

/// Yield the CPU without issuing a host syscall in tests.
fn relax() {
    #[cfg(target_os = "none")]
    {
        sys::yield_now().ok();
    }
    #[cfg(not(target_os = "none"))]
    {
        core::hint::spin_loop();
    }
}

/// Wait on the futex-shaped writer word; the host spins so unit tests never issue a Raven
/// syscall number to Linux (0x22 is `pause` there).
fn wait_word(word: &AtomicU32) {
    #[cfg(target_os = "none")]
    {
        sys::futex_wait(word, 1, 0).ok();
    }
    #[cfg(not(target_os = "none"))]
    {
        let _ = word;
        core::hint::spin_loop();
    }
}

/// Wake waiters on the writer word.
fn wake_word(word: &AtomicU32) {
    #[cfg(target_os = "none")]
    {
        sys::futex_wake(word, u32::MAX).ok();
    }
    #[cfg(not(target_os = "none"))]
    {
        let _ = word;
    }
}

/// Mask the internal signals out of an application mask word.
#[must_use]
pub const fn app_mask_word(word: u64) -> u64 {
    word & crate::sigset::APP_MASK
}

const _: () = assert!(ENTRY_COUNT == crate::sigset::NSIG);
const _: () = assert!(EINVAL_ERR.0 == 22);

#[cfg(test)]
#[path = "dispositions_ut.rs"]
mod dispositions_ut;
