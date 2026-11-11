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
//! Owner protocol (current execution model):
//!
//! * [`allocate`] claims the bitmap bit.
//! * [`install_owner`] publishes the typed owner pointer with `Release` before the interrupt
//!   source is armed, so the handler sees either null or a live object.
//! * [`release`] and [`quarantine`] clear the owner with `Release` *before* the slot is made
//!   available again.  A quarantined vector keeps its bitmap bit forever, so a still-enabled
//!   device cannot deliver an old message to a new `Irq`.
//! * The handler reads the owner with `Acquire` and only dereferences a non-null pointer.
//!
//! The slot does not own a reference to the object.  The lifetime argument is the execution
//! model: the kernel is non-preemptible, user threads and process-context closes run on CPU 0,
//! and device delivery is routed to that same CPU.  The handler therefore cannot run while
//! `Irq::drop` clears the slot, and the object stays alive until `drop` returns.  Enabling
//! user-thread SMP, kernel preemption or delivery on another CPU invalidates the argument and
//! needs an owning reference or a different reclamation protocol.

use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

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
    true
}

fn clear_owner(index: usize) {
    if let Some(owner_slot) = OWNERS.get(index) {
        owner_slot.store(ptr::null_mut(), Ordering::Release);
    }
}

/// The owner pointer of a device vector, or null when it is unowned or not in the pool.
///
/// Called from the interrupt handler, which never blocks.  The returned pointer must only be
/// dereferenced while the execution model keeps `Irq::drop` from running on another CPU; see the
/// module documentation.
pub fn owner(vector: u8) -> *const Irq {
    match slot(vector) {
        Some(index) => OWNERS.get(index).map_or(ptr::null(), |slot| {
            slot.load(Ordering::Acquire).cast_const()
        }),
        None => ptr::null(),
    }
}
