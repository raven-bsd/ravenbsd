// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from DragonFlyBSD `sys/kern/lwkt_ipiq.c` and `sys/sys/thread.h`:
// `struct lwkt_ipiq`, `MAXCPUFIFO`/`MAXCPUFIFO_MASK`, `lwkt_send_ipiq3` and
// `lwkt_process_ipiq_core`. Upstream licence: BSD-3-Clause (The DragonFly
// Project); see `LICENSE-DragonFly` and the repository `PROVENANCE.toml`.

//! Per-CPU IPI message FIFO.
//!
//! One FIFO exists for every (source CPU, target CPU) pair. The source writes
//! `windex` and the message slot; the target owns `rindex`/`xindex`. The ring is
//! the DragonFly `MAXCPUFIFO` power-of-two array with one slot left free, and
//! `process(limit)` mirrors `lwkt_process_ipiq_core`: it caches `windex`, drains
//! at most the requested number, advances `rindex` before the callback and
//! publishes `xindex` after it.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(unsafe_code)] // The ring slots are single-producer/single-consumer UnsafeCells.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// DragonFly `MAXCPUFIFO` (power of two).
pub const MAXCPUFIFO: usize = 256;
/// DragonFly `MAXCPUFIFO_MASK`.
pub const MAXCPUFIFO_MASK: usize = MAXCPUFIFO - 1;

/// One queued IPI request: `struct ip_info`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message<T> {
    /// The target function (`ipifunc3_t` in the donor).
    pub func: T,
    /// First argument.
    pub arg1: usize,
    /// Second argument.
    pub arg2: i32,
}

/// One per-(source,target) IPI FIFO (`struct lwkt_ipiq`).
pub struct IpIQ<T> {
    rindex: AtomicU32,
    xindex: AtomicU32,
    windex: AtomicU32,
    drain: AtomicI32,
    info: [UnsafeCell<MaybeUninit<Message<T>>>; MAXCPUFIFO],
}

// SAFETY: the FIFO is single-producer/single-consumer for the message slots:
// `push` is the source and advances `windex`, `pop`/`process` is the target and
// advances `rindex`/`xindex`. The atomics publish the slot contents.
unsafe impl<T: Send> Sync for IpIQ<T> {}

impl<T> IpIQ<T> {
    /// Create an empty FIFO.
    pub const fn new() -> Self {
        Self {
            rindex: AtomicU32::new(0),
            xindex: AtomicU32::new(0),
            windex: AtomicU32::new(0),
            drain: AtomicI32::new(0),
            info: [const { UnsafeCell::new(MaybeUninit::uninit()) }; MAXCPUFIFO],
        }
    }

    /// Queue one message (source CPU). Returns the new `windex`, or the message
    /// when the FIFO has no free slot (one slot is kept free, as in the donor).
    pub fn push(&self, message: Message<T>) -> Result<u32, Message<T>> {
        let windex = self.windex.load(Ordering::Relaxed);
        let rindex = self.rindex.load(Ordering::Acquire);
        // One slot is left free, exactly as the donor's MAXCPUFIFO - 1 invariant.
        if windex.wrapping_sub(rindex) as usize >= MAXCPUFIFO - 1 {
            return Err(message);
        }
        let slot = (windex as usize) & MAXCPUFIFO_MASK;
        // SAFETY: `push` is the single producer and the free-slot check above
        // guarantees the target has consumed this slot (rindex is behind it).
        unsafe {
            (*self.info[slot].get()).write(message);
        }
        core::sync::atomic::fence(Ordering::Release);
        let next = windex.wrapping_add(1);
        self.windex.store(next, Ordering::Release);
        Ok(next)
    }

    /// Drain up to `limit` messages (`0` = all currently visible), calling `call`
    /// for each after `rindex` advances. Returns the number still pending.
    pub fn process(&self, limit: usize, mut call: impl FnMut(Message<T>)) -> u32 {
        // Cache the source index: a callback may queue more and advance windex,
        // but this pass may only read slots the source published before the fence.
        let windex = self.windex.load(Ordering::Acquire);
        let mut rindex = self.rindex.load(Ordering::Relaxed);
        while windex.wrapping_sub(rindex) as usize > limit {
            let slot = (rindex as usize) & MAXCPUFIFO_MASK;
            // SAFETY: the target owns rindex; the source published the slot
            // before storing windex. The message is copied out before rindex
            // advances, so the source may reuse the slot during `call`.
            let message = unsafe { (*self.info[slot].get()).assume_init_read() };
            rindex = rindex.wrapping_add(1);
            self.rindex.store(rindex, Ordering::Release);
            call(message);
            // xindex is only published after the callback finished (as in the donor).
            self.xindex.store(rindex, Ordering::Release);
        }
        self.windex.load(Ordering::Acquire).wrapping_sub(rindex)
    }

    /// Pop one message without a callback, publishing `xindex`.
    pub fn pop(&self) -> Option<Message<T>> {
        let rindex = self.rindex.load(Ordering::Relaxed);
        let windex = self.windex.load(Ordering::Acquire);
        if rindex == windex {
            return None;
        }
        let slot = (rindex as usize) & MAXCPUFIFO_MASK;
        // SAFETY: same single-target discipline as `process`.
        let message = unsafe { (*self.info[slot].get()).assume_init_read() };
        let next = rindex.wrapping_add(1);
        self.rindex.store(next, Ordering::Release);
        self.xindex.store(next, Ordering::Release);
        Some(message)
    }

    /// Messages queued but not yet processed.
    pub fn pending(&self) -> u32 {
        self.windex
            .load(Ordering::Acquire)
            .wrapping_sub(self.rindex.load(Ordering::Acquire))
    }

    /// Sequence number of the last completed message.
    pub fn xindex(&self) -> u32 {
        self.xindex.load(Ordering::Acquire)
    }

    /// Source write index.
    pub fn windex(&self) -> u32 {
        self.windex.load(Ordering::Acquire)
    }

    /// Target read index.
    pub fn rindex(&self) -> u32 {
        self.rindex.load(Ordering::Acquire)
    }

    /// Drain hysteresis value (`ip_drain`).
    pub fn drain(&self) -> i32 {
        self.drain.load(Ordering::Acquire)
    }

    /// Replace the drain hysteresis value.
    pub fn set_drain(&self, value: i32) {
        self.drain.store(value, Ordering::Release);
    }
}

impl<T> Default for IpIQ<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
