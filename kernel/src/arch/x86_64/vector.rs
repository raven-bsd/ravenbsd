// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver interrupt vector pool.
//!
//! Device vectors live above the exception (0..32), timer (0x20/0x21) and IPI (0x30..0x32)
//! vectors.  Each vector has exactly one owner (the `Irq` object); closing the object returns
//! the vector to the pool.  The bitmap is lock-free because allocation and release only happen
//! in process context, while the interrupt handler only reads the owner table.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::msi::vector_in_pool;

/// First device vector.
pub const VECTOR_BASE: u8 = 0x40;
/// Number of device vectors (0x40..=0xBF).
pub const VECTOR_COUNT: u8 = 128;

/// Allocation bitmap: one bit per device vector.
static TAKEN: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// Owner of one device vector: the raw `Arc<Irq>` pointer, or null.
static OWNERS: [AtomicU64; VECTOR_COUNT as usize] =
    [const { AtomicU64::new(0) }; VECTOR_COUNT as usize];

/// True when `vector` belongs to the device pool.
pub fn is_driver_vector(vector: u8) -> bool {
    vector_in_pool(vector, VECTOR_BASE, VECTOR_COUNT)
}

fn slot(vector: u8) -> Option<usize> {
    is_driver_vector(vector).then(|| usize::from(vector.wrapping_sub(VECTOR_BASE)))
}

/// Allocate a free vector. Returns `None` when the pool is exhausted.
pub fn allocate() -> Option<u8> {
    let mut index = 0usize;
    while index < VECTOR_COUNT as usize {
        let word = index / 64;
        let bit = (index % 64) as u32;
        let mask = 1u64 << bit;
        let slot = TAKEN.get(word)?;
        let previous = slot.fetch_or(mask, Ordering::AcqRel);
        if previous & mask == 0 {
            return Some(VECTOR_BASE.wrapping_add(index as u8));
        }
        index = index.saturating_add(1);
    }
    None
}

/// Publish the raw `Arc<Irq>` pointer owning `vector`.
///
/// Called after [`allocate`] and before the source can fire, so the interrupt handler always
/// finds either the valid owner or an empty slot.
pub fn install_owner(vector: u8, owner: u64) {
    if let Some(index) = slot(vector)
        && let Some(owner_slot) = OWNERS.get(index)
    {
        owner_slot.store(owner, Ordering::Release);
    }
}

/// Return a vector to the pool and clear its owner.
pub fn release(vector: u8) -> bool {
    let Some(index) = slot(vector) else {
        return false;
    };
    let word = index / 64;
    let bit = (index % 64) as u32;
    let mask = 1u64 << bit;
    if let Some(slot) = TAKEN.get(word) {
        slot.fetch_and(!mask, Ordering::AcqRel);
    }
    if let Some(owner_slot) = OWNERS.get(index) {
        owner_slot.store(0, Ordering::Release);
    }
    true
}

/// The raw owner pointer of a device vector, or null when it is unowned or not in the pool.
///
/// Called from the interrupt handler, which never blocks: on the boot CPU the owner cannot
/// disappear while this function and its caller run, because release happens in process context
/// on the same CPU.
pub fn owner(vector: u8) -> u64 {
    match slot(vector) {
        Some(index) => OWNERS
            .get(index)
            .map_or(0, |slot| slot.load(Ordering::Acquire)),
        None => 0,
    }
}
