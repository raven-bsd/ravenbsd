// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deferred signals in libc critical sections (A-sol-20, CLEANROOM from the prose of `06`).
//!
//! A handler is never called from `critical_exit`: the signal is remembered in the thread's
//! `sig_private` block and replayed when the counter pair reaches zero, through `SIGRESEND` when
//! the kernel offers it (PG-7) or through "restore mask, set the pending bit, kick self" (the
//! documented race). A new thread starts with `defer = 1` (A-sol-23) and the first thread code
//! clears it after TLS initialisation.

use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_libc_abi::structs::SiginfoT;

use crate::SigKernel;
use crate::dispositions::DispTable;
use crate::raw;
use crate::sigset::{APP_MASK, SigSet};

/// The per-thread signal state, a typed view over `Pthread.sig_private` (32 words = 256 bytes).
///
/// One `SiginfoT` slot serves both the pending and the deferred record: the ticket's two 128-byte
/// slots would exceed the reserved block. A second arrival overwrites the record; the bit set is
/// the authority for delivery, exactly as the one-slot drop rule of 4.7 describes.
#[repr(C)]
pub struct SigThreadState {
    critical: AtomicU32,
    defer: AtomicU32,
    deferred_bits: AtomicU64,
    deferred_info: SiginfoT,
    deferred_mask: AtomicU64,
    in_handler: AtomicU32,
    saved_errno: AtomicI32,
    ctx_area: AtomicUsize,
    kick: AtomicU32,
}

impl SigThreadState {
    /// A zeroed state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            critical: AtomicU32::new(0),
            defer: AtomicU32::new(0),
            deferred_bits: AtomicU64::new(0),
            deferred_info: SiginfoT::ZERO,
            deferred_mask: AtomicU64::new(0),
            in_handler: AtomicU32::new(0),
            saved_errno: AtomicI32::new(0),
            ctx_area: AtomicUsize::new(0),
            kick: AtomicU32::new(0),
        }
    }

    /// The critical-section nesting count.
    #[must_use]
    pub fn critical(&self) -> u32 {
        self.critical.load(Ordering::Acquire)
    }

    /// The explicit-deferral count.
    #[must_use]
    pub fn defer(&self) -> u32 {
        self.defer.load(Ordering::Acquire)
    }

    /// The deferred signal bits.
    #[must_use]
    pub fn deferred_bits(&self) -> u64 {
        self.deferred_bits.load(Ordering::Acquire)
    }

    /// The deferred record.
    #[must_use]
    pub fn deferred_info(&self) -> &SiginfoT {
        &self.deferred_info
    }

    /// The interrupted mask saved with a deferred signal.
    #[must_use]
    pub fn deferred_mask(&self) -> u64 {
        self.deferred_mask.load(Ordering::Acquire)
    }

    /// Whether the thread is executing a handler.
    #[must_use]
    pub fn in_handler(&self) -> u32 {
        self.in_handler.load(Ordering::Acquire)
    }

    /// Save/restore state for the entry path.
    #[must_use]
    pub fn saved_errno(&self) -> i32 {
        self.saved_errno.load(Ordering::Acquire)
    }

    /// The registered context area (0 = none).
    #[must_use]
    pub fn ctx_area(&self) -> usize {
        self.ctx_area.load(Ordering::Acquire)
    }

    /// The sticky kick word used until PG-8 exists.
    #[must_use]
    pub fn kick(&self) -> u32 {
        self.kick.load(Ordering::Acquire)
    }

    pub(crate) fn set_ctx_area(&mut self, address: usize) {
        self.ctx_area.store(address, Ordering::Release);
    }

    pub(crate) fn take_kick(&mut self) -> u32 {
        self.kick.swap(0, Ordering::AcqRel)
    }

    pub(crate) fn set_info(&mut self, value: &SiginfoT) {
        self.deferred_info = *value;
    }

    #[must_use]
    pub(crate) fn info(&self) -> &SiginfoT {
        &self.deferred_info
    }

    pub(crate) fn set_saved_errno(&mut self, value: i32) {
        self.saved_errno.store(value, Ordering::Release);
    }

    pub(crate) fn add_critical(&mut self) {
        let old = self.critical.load(Ordering::Relaxed);
        self.critical
            .store(old.saturating_add(1), Ordering::Release);
    }

    pub(crate) fn sub_critical(&mut self) -> u32 {
        let old = self.critical.load(Ordering::Relaxed);
        let next = old.saturating_sub(1);
        self.critical.store(next, Ordering::Release);
        next
    }

    pub(crate) fn add_defer(&mut self) {
        let old = self.defer.load(Ordering::Relaxed);
        self.defer.store(old.saturating_add(1), Ordering::Release);
    }

    pub(crate) fn sub_defer(&mut self) -> u32 {
        let old = self.defer.load(Ordering::Relaxed);
        let next = old.saturating_sub(1);
        self.defer.store(next, Ordering::Release);
        next
    }
}

impl Default for SigThreadState {
    fn default() -> Self {
        Self::new()
    }
}

/// Enter a critical section (`critical += 1`).
pub fn critical_enter<K: SigKernel>(kernel: &K) {
    let tp = kernel.thread_pointer();
    raw::with_state(tp, |state| state.add_critical());
}

/// Leave a critical section; when both counters reach zero, replay a deferred signal.
pub fn critical_exit<K: SigKernel>(kernel: &K, table: &DispTable) {
    let tp = kernel.thread_pointer();
    let (critical, defer, bits) = raw::with_state(tp, |state| {
        let critical = state.sub_critical();
        (
            critical,
            state.defer.load(Ordering::Acquire),
            state.deferred_bits.load(Ordering::Acquire),
        )
    });
    if critical == 0 && defer == 0 && bits != 0 {
        replay(kernel, table);
    }
}

/// Enter an explicit deferral region.
pub fn defer_enter<K: SigKernel>(kernel: &K) {
    let tp = kernel.thread_pointer();
    raw::with_state(tp, |state| state.add_defer());
}

/// Leave an explicit deferral region; replay when both counters are zero.
pub fn defer_exit<K: SigKernel>(kernel: &K, table: &DispTable) {
    let tp = kernel.thread_pointer();
    let (critical, defer, bits) = raw::with_state(tp, |state| {
        let defer = state.sub_defer();
        (
            state.critical.load(Ordering::Acquire),
            defer,
            state.deferred_bits.load(Ordering::Acquire),
        )
    });
    if critical == 0 && defer == 0 && bits != 0 {
        replay(kernel, table);
    }
}

/// A new thread starts with `defer = 1` (A-sol-23); its first code clears it.
pub fn init_thread_state(tp: usize) {
    raw::with_state(tp, |state| {
        *state = SigThreadState::new();
        state.defer.store(1, Ordering::Release);
    });
}

/// Record a deferred delivery and the interrupted mask (step 6 of the entry path).
pub fn defer_signal<K: SigKernel>(kernel: &K, sig: i32, info: &SiginfoT, mask: u64) {
    let tp = kernel.thread_pointer();
    let shift = (sig.wrapping_sub(1) & 63) as u32;
    raw::with_state(tp, |state| {
        state.deferred_info = *info;
        state.deferred_mask.store(mask, Ordering::Release);
        state
            .deferred_bits
            .fetch_or(1u64.wrapping_shl(shift), Ordering::AcqRel);
    });
}

/// Replay one deferred signal. The handler is never called here.
pub fn replay<K: SigKernel>(kernel: &K, _table: &DispTable) {
    let tp = kernel.thread_pointer();
    let (bits, info, mask) = raw::with_state(tp, |state| {
        (
            state.deferred_bits.load(Ordering::Acquire),
            state.deferred_info,
            state.deferred_mask.load(Ordering::Acquire),
        )
    });
    if bits == 0 {
        return;
    }
    let sig = lowest_bit(bits);
    if sig == 0 {
        return;
    }
    if kernel.caps().resend && kernel.resend(sig, &info, mask).is_ok() {
        clear_deferred_bits(
            tp,
            bits & 1u64.wrapping_shl((sig.wrapping_sub(1) & 63) as u32),
        );
        return;
    }
    // Fallback: restore the interrupted mask, set the pending bit again and kick. The window
    // between the mask store and the kick is the documented race; a stress test covers it.
    let bit = 1u64.wrapping_shl((sig.wrapping_sub(1) & 63) as u32);
    raw::with_state(tp, |state| {
        state.deferred_bits.fetch_and(!bit, Ordering::AcqRel);
    });
    raw::pending_or(tp, bit);
    raw::mask_store(tp, mask & APP_MASK);
    let handle = kernel.thread_handle();
    kernel.kick(handle, crate::kernel::KICK_CHECK).ok();
}

fn clear_deferred_bits(tp: usize, clear: u64) {
    raw::with_state(tp, |state| {
        state.deferred_bits.fetch_and(!clear, Ordering::AcqRel);
    });
}

fn lowest_bit(bits: u64) -> i32 {
    if bits == 0 {
        return 0;
    }
    let index = bits.trailing_zeros();
    if index >= 64 {
        0
    } else {
        (index as i32).wrapping_add(1)
    }
}

/// The bitset form of the mask of a single signal.
#[must_use]
pub fn signal_bit(sig: i32) -> u64 {
    if !(1..=64).contains(&sig) {
        return 0;
    }
    1u64.wrapping_shl((sig.wrapping_sub(1) & 63) as u32)
}

/// Set the handler nesting marker used by diagnostics.
pub fn set_in_handler(tp: usize, active: bool) {
    raw::with_state(tp, |state| {
        state.in_handler.store(u32::from(active), Ordering::Release);
    });
}

/// The current thread's signal state copy (diagnostics and tests).
#[must_use]
pub fn snapshot(tp: usize) -> (u32, u32, u64, u32, i32) {
    raw::with_state(tp, |state| {
        (
            state.critical(),
            state.defer(),
            state.deferred_bits(),
            state.in_handler(),
            state.saved_errno(),
        )
    })
}

/// A mask with every application signal blocked.
#[must_use]
pub fn app_mask() -> SigSet {
    let mut set = SigSet::empty();
    set.fill();
    set
}

const _: () =
    assert!(core::mem::size_of::<SigThreadState>() <= raven_libc_tls::tcb::SIG_PRIVATE_BYTES);

#[cfg(test)]
#[path = "critical_ut.rs"]
mod critical_ut;
