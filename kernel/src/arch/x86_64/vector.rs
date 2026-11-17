// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver interrupt vector pool.
//!
//! Device vectors live above the exception (0..32), timer (0x20/0x21) and IPI (0x30..0x32)
//! vectors.  Each vector has exactly one owner (the `Irq` object); closing the object returns
//! the vector to the pool or quarantines it.  The bitmap and the owner table are lock-free
//! because allocation, owner publication and release only happen in process context, while the
//! interrupt handler only reads the owner table.
//!
//! Owner protocol (active readers):
//!
//! * [`allocate`] claims the bitmap bit.
//! * [`install_owner`] publishes the typed owner pointer with `Release` before the interrupt
//!   source is armed, so the handler sees either null or a live object.
//! * [`release`] and [`quarantine`] clear the owner with `Release` *before* the slot is made
//!   available again.  A quarantined vector keeps its bitmap bit forever, so a still-enabled
//!   device cannot deliver an old message to a new `Irq`.
//! * The handler acquires a per-vector reader slot **before** loading the owner, then clones an
//!   `Arc` from the pointer.  `release`/`quarantine` clear the owner and then wait for the reader
//!   count to reach zero before returning, so the object can never be freed while a handler (on
//!   the same or another CPU) still dereferences it.
//!
//! The slot itself does not own a reference: the `Irq` is kept alive by the process handle table
//! and by each in-flight handler's cloned `Arc`.  This protocol is what makes external-IRQ
//! delivery on an application processor (S75a) safe without a scheduler change.

use alloc::sync::Arc;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};

use crate::arch::x86_64::msi::vector_in_pool;
use crate::driver::irq::Irq;

/// First device vector.
pub const VECTOR_BASE: u8 = 0x40;
/// Number of device vectors (0x40..=0xBF).
pub const VECTOR_COUNT: u8 = 128;

/// Allocation bitmap: one bit per device vector.
static TAKEN: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// Owner of one device vector: a typed pointer to the `Irq`, or null.
static OWNERS: [AtomicPtr<Irq>; VECTOR_COUNT as usize] =
    [const { AtomicPtr::new(ptr::null_mut()) }; VECTOR_COUNT as usize];
/// In-flight handlers per vector (active-reader protocol).
static READERS: [AtomicU32; VECTOR_COUNT as usize] =
    [const { AtomicU32::new(0) }; VECTOR_COUNT as usize];

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

/// Publish the owner pointer of `vector`.
///
/// Called after [`allocate`] and before the source can fire, so the interrupt handler always
/// finds either the live object or an empty slot.  The caller keeps the object alive until
/// [`release`] or [`quarantine`] clears the slot (see the module documentation).
pub fn install_owner(vector: u8, owner: *const Irq) {
    if let Some(index) = slot(vector)
        && let Some(owner_slot) = OWNERS.get(index)
    {
        owner_slot.store(owner.cast_mut(), Ordering::Release);
    }
}

/// Return a vector to the pool.
///
/// The owner is cleared before the allocation bit is published as free.  The reverse order
/// would let the next allocator install its owner and then have this release erase it, leaving a
/// live interrupt source with no owner.
pub fn release(vector: u8) -> bool {
    let Some(index) = slot(vector) else {
        return false;
    };
    clear_owner(index);
    wait_readers(index);
    let word = index / 64;
    let bit = (index % 64) as u32;
    let mask = 1u64 << bit;
    if let Some(slot) = TAKEN.get(word) {
        slot.fetch_and(!mask, Ordering::AcqRel);
    }
    true
}

/// Retire a vector whose source could not be proven quiescent.
///
/// The owner is cleared (late messages are dropped as unclaimed) and the allocation bit stays
/// set, so the vector is never handed out again.  This is the fail-safe path: a driver that
/// closes a live MSI source without the documented final `IRQ_ACK` costs one vector instead of
/// letting an old device message reach a new `Irq`.
pub fn quarantine(vector: u8) -> bool {
    let Some(index) = slot(vector) else {
        return false;
    };
    clear_owner(index);
    wait_readers(index);
    true
}

fn clear_owner(index: usize) {
    if let Some(owner_slot) = OWNERS.get(index) {
        owner_slot.store(ptr::null_mut(), Ordering::Release);
    }
}

/// Spin until no handler is between its reader acquire and its reader release.
///
/// `clear_owner` already ran, so no new handler can pass the owner load; only readers that
/// incremented before the clear remain.  The handler never blocks, so this terminates.
fn wait_readers(index: usize) {
    if let Some(reader_slot) = READERS.get(index) {
        while reader_slot.load(Ordering::Acquire) != 0 {
            core::hint::spin_loop();
        }
    }
}

/// Acquire an owning reference to the owner of a device vector, or `None`.
///
/// Called from the interrupt handler, which never blocks.  The reader slot is taken before the
/// owner load, so [`release`]/[`quarantine`] wait for this handler before returning; the cloned
/// `Arc` then keeps the `Irq` alive for the rest of the handler even if the slot is cleared.
pub fn handle_owner(vector: u8) -> Option<Arc<Irq>> {
    let index = slot(vector)?;
    let owner_slot = OWNERS.get(index)?;
    let reader_slot = READERS.get(index)?;
    reader_slot.fetch_add(1, Ordering::AcqRel);
    let raw = owner_slot.load(Ordering::Acquire);
    if raw.is_null() {
        reader_slot.fetch_sub(1, Ordering::AcqRel);
        return None;
    }
    // SAFETY: the reader slot is held, so `release`/`quarantine` cannot free the object until
    // this handler releases it; `raw` is therefore a live `Arc<Irq>` inner.  Clone one reference.
    unsafe { Arc::increment_strong_count(raw) };
    // SAFETY: `increment_strong_count` made this a valid owning reference.
    let irq = unsafe { Arc::from_raw(raw) };
    reader_slot.fetch_sub(1, Ordering::AcqRel);
    Some(irq)
}
