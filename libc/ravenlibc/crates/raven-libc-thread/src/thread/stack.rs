// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread stacks: one mapping per thread, the guard, the LIFO cache and the zombie list.
//!
//! The cache holds at most [`STACK_CACHE_MAX`] default-size stacks. A cached stack is only
//! reused when its previous owner is provably gone; the joiner never frees on a user-space flag
//! alone (A-thr-3). Without `exit_ex` a detached thread pushes its stack on the zombie list and
//! the next create/join/exit reaps entries whose `exit_word` is set after one yield.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{EAGAIN, EINVAL, Errno, STACK_CACHE_MAX, ZOMBIE_MAX};
use crate::raw;

const PAGE: usize = 4096;

fn round_up(value: usize, align: usize) -> Option<usize> {
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|sum| sum & !mask)
}

fn round_down(value: usize, align: usize) -> usize {
    value & !align.wrapping_sub(1)
}

/// One reserved stack mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackAlloc {
    /// Base of the mapping that `unmap`/`protect_none` take (zero for a user stack).
    pub map_base: usize,
    /// Length of that mapping (zero for a user stack).
    pub map_size: usize,
    /// Guard length at the low end (zero for a user stack).
    pub guard: usize,
    /// Low address of the usable stack.
    pub stack_limit: usize,
    /// High address of the usable stack (16-byte aligned).
    pub stack_top: usize,
    /// Base of the TLS block.
    pub tls_base: usize,
    /// Base of the TSD array.
    pub tsd_base: usize,
    /// Reserved TLS bytes.
    pub tls_size: usize,
    /// Reserved TSD bytes.
    pub tsd_size: usize,
}

static CACHE_LOCK: AtomicBool = AtomicBool::new(false);
static CACHE_BASE: [AtomicUsize; STACK_CACHE_MAX] =
    [const { AtomicUsize::new(0) }; STACK_CACHE_MAX];
static CACHE_SIZE: [AtomicUsize; STACK_CACHE_MAX] =
    [const { AtomicUsize::new(0) }; STACK_CACHE_MAX];
static CACHE_LEN: AtomicUsize = AtomicUsize::new(0);

static ZOMBIE_LOCK: AtomicBool = AtomicBool::new(false);
static ZOMBIE_BASE: [AtomicUsize; ZOMBIE_MAX] = [const { AtomicUsize::new(0) }; ZOMBIE_MAX];
static ZOMBIE_SIZE: [AtomicUsize; ZOMBIE_MAX] = [const { AtomicUsize::new(0) }; ZOMBIE_MAX];
static ZOMBIE_OWNER: [AtomicUsize; ZOMBIE_MAX] = [const { AtomicUsize::new(0) }; ZOMBIE_MAX];
static ZOMBIE_LEN: AtomicUsize = AtomicUsize::new(0);

fn spin_lock(flag: &AtomicBool) {
    while flag
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

fn spin_unlock(flag: &AtomicBool) {
    flag.store(false, Ordering::Release);
}

/// Empty both caches (startup and each serialised host test).
pub fn reset() {
    CACHE_LEN.store(0, Ordering::Release);
    ZOMBIE_LEN.store(0, Ordering::Release);
}

/// Number of cached stacks.
#[must_use]
pub fn cached() -> usize {
    CACHE_LEN.load(Ordering::Acquire)
}

/// Number of zombie stacks.
#[must_use]
pub fn zombies() -> usize {
    ZOMBIE_LEN.load(Ordering::Acquire)
}

/// Push a freed default-size stack on the cache, or unmap it when it does not belong there.
///
/// # Errors
///
/// `EAGAIN` when the zombie list is full.
pub fn release<K: ThreadKernel>(kernel: &K, alloc: StackAlloc) -> Result<(), Errno> {
    if alloc.map_base == 0 || alloc.map_size == 0 {
        return Ok(());
    }
    if alloc.map_size
        == alloc
            .guard
            .wrapping_add(round_up_or_zero(crate::limits::DEFAULT_STACK_SIZE))
    {
        spin_lock(&CACHE_LOCK);
        let length = CACHE_LEN.load(Ordering::Acquire);
        if length < STACK_CACHE_MAX {
            if let (Some(base), Some(size)) = (CACHE_BASE.get(length), CACHE_SIZE.get(length)) {
                base.store(alloc.map_base, Ordering::Release);
                size.store(alloc.map_size, Ordering::Release);
            }
            CACHE_LEN.store(length.wrapping_add(1), Ordering::Release);
            spin_unlock(&CACHE_LOCK);
            return Ok(());
        }
        spin_unlock(&CACHE_LOCK);
    }
    kernel.unmap(alloc.map_base, alloc.map_size)
}

fn round_up_or_zero(value: usize) -> usize {
    round_up(value, PAGE).unwrap_or(0)
}

/// Pop a cached stack of exactly `map_size`.
#[must_use]
pub fn acquire_cached(map_size: usize) -> Option<StackAlloc> {
    spin_lock(&CACHE_LOCK);
    let mut length = CACHE_LEN.load(Ordering::Acquire);
    let mut found = None;
    while length > 0 {
        length -= 1;
        let size = CACHE_SIZE
            .get(length)
            .map_or(0, |slot| slot.load(Ordering::Acquire));
        if size == map_size {
            let base = CACHE_BASE
                .get(length)
                .map_or(0, |slot| slot.load(Ordering::Acquire));
            if let (Some(base_slot), Some(size_slot)) =
                (CACHE_BASE.get(length), CACHE_SIZE.get(length))
            {
                base_slot.store(0, Ordering::Release);
                size_slot.store(0, Ordering::Release);
            }
            CACHE_LEN.store(length, Ordering::Release);
            found = Some(base);
            break;
        }
    }
    spin_unlock(&CACHE_LOCK);
    found.map(|base| StackAlloc {
        map_base: base,
        map_size,
        guard: 0,
        stack_limit: base,
        stack_top: base.wrapping_add(map_size),
        tls_base: 0,
        tsd_base: 0,
        tls_size: 0,
        tsd_size: 0,
    })
}

/// Push a detached thread's stack on the zombie list.
///
/// # Errors
///
/// `EAGAIN` when the list is full (the cap is the documented leak bound).
pub fn push_zombie(base: usize, size: usize, owner: usize) -> Result<(), Errno> {
    spin_lock(&ZOMBIE_LOCK);
    let length = ZOMBIE_LEN.load(Ordering::Acquire);
    if length >= ZOMBIE_MAX {
        spin_unlock(&ZOMBIE_LOCK);
        return Err(EAGAIN);
    }
    if let (Some(base_slot), Some(size_slot), Some(owner_slot)) = (
        ZOMBIE_BASE.get(length),
        ZOMBIE_SIZE.get(length),
        ZOMBIE_OWNER.get(length),
    ) {
        base_slot.store(base, Ordering::Release);
        size_slot.store(size, Ordering::Release);
        owner_slot.store(owner, Ordering::Release);
    }
    ZOMBIE_LEN.store(length.wrapping_add(1), Ordering::Release);
    spin_unlock(&ZOMBIE_LOCK);
    Ok(())
}

/// Reap zombies whose owner's `exit_word` is set after one yield.
pub fn reap<K: ThreadKernel>(kernel: &K) {
    kernel.yield_now();
    let mut index = 0usize;
    let mut write = 0usize;
    loop {
        let length = ZOMBIE_LEN.load(Ordering::Acquire);
        if index >= length {
            break;
        }
        let base = ZOMBIE_BASE
            .get(index)
            .map_or(0, |slot| slot.load(Ordering::Acquire));
        let size = ZOMBIE_SIZE
            .get(index)
            .map_or(0, |slot| slot.load(Ordering::Acquire));
        let owner = ZOMBIE_OWNER
            .get(index)
            .map_or(0, |slot| slot.load(Ordering::Acquire));
        index += 1;
        let exited = owner != 0 && raw::tcb(owner).exit_word.load(Ordering::Acquire) == 1;
        if exited {
            let _ = kernel.unmap(base, size);
        } else {
            if write != index - 1
                && let (Some(base_slot), Some(size_slot), Some(owner_slot)) = (
                    ZOMBIE_BASE.get(write),
                    ZOMBIE_SIZE.get(write),
                    ZOMBIE_OWNER.get(write),
                )
            {
                base_slot.store(base, Ordering::Release);
                size_slot.store(size, Ordering::Release);
                owner_slot.store(owner, Ordering::Release);
            }
            write += 1;
        }
    }
    ZOMBIE_LEN.store(write, Ordering::Release);
}

/// Reserve the mapping for one thread.
///
/// # Errors
///
/// `EINVAL` for an impossible user stack, `EAGAIN` when the kernel cannot map.
pub fn allocate<K: ThreadKernel>(
    kernel: &K,
    stack_size: usize,
    guard_size: usize,
    user_stack: Option<(usize, usize)>,
    tls_size: usize,
    tsd_size: usize,
) -> Result<StackAlloc, Errno> {
    let need = tls_size.checked_add(tsd_size).ok_or(EINVAL)?;
    if let Some((address, size)) = user_stack {
        if size < crate::limits::PTHREAD_STACK_MIN || address == 0 {
            return Err(EINVAL);
        }
        let top = address.wrapping_add(size);
        if need < size / 8 && need < 2048 {
            let tsd_base = round_down(top.wrapping_sub(tsd_size), 16);
            let tls_base = round_down(tsd_base.wrapping_sub(tls_size), 16);
            Ok(StackAlloc {
                map_base: 0,
                map_size: 0,
                guard: 0,
                stack_limit: address,
                stack_top: tls_base & !15,
                tls_base,
                tsd_base,
                tls_size,
                tsd_size,
            })
        } else {
            let map_size = round_up(need, PAGE).ok_or(EINVAL)?;
            let base = kernel.map(map_size)?;
            Ok(StackAlloc {
                map_base: base,
                map_size,
                guard: 0,
                stack_limit: address,
                stack_top: round_down(top, 16),
                tls_base: base,
                tsd_base: base.wrapping_add(tls_size),
                tls_size,
                tsd_size,
            })
        }
    } else {
        let guard = round_up(guard_size, PAGE).ok_or(EINVAL)?;
        let body = round_up(stack_size.checked_add(need).ok_or(EINVAL)?, PAGE).ok_or(EINVAL)?;
        let map_size = guard.checked_add(body).ok_or(EINVAL)?;
        let cached = acquire_cached(map_size);
        let base = match cached {
            Some(cached) => {
                kernel.protect_none(cached.map_base, guard)?;
                cached.map_base
            }
            None => {
                let base = kernel.map(map_size)?;
                if guard != 0 && kernel.protect_none(base, guard).is_err() {
                    let _ = kernel.unmap(base, map_size);
                    return Err(EAGAIN);
                }
                base
            }
        };
        let tsd_base = base.wrapping_add(map_size).wrapping_sub(tsd_size);
        let tls_base = round_down(tsd_base.wrapping_sub(tls_size), 16);
        Ok(StackAlloc {
            map_base: base,
            map_size,
            guard,
            stack_limit: base.wrapping_add(guard),
            stack_top: tls_base & !15,
            tls_base,
            tsd_base,
            tls_size,
            tsd_size,
        })
    }
}

/// Unmap a mapping that was never published as a thread.
pub fn abort<K: ThreadKernel>(kernel: &K, alloc: StackAlloc) {
    if alloc.map_base != 0 && alloc.map_size != 0 {
        let _ = kernel.unmap(alloc.map_base, alloc.map_size);
    }
}

#[cfg(test)]
#[path = "stack_ut.rs"]
mod tests;
