// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The stateful enumerator of `getpwent`/`getgrent`: one owned `Vec` and a position, stored as
//! one pointer in the C64 scratch area. The load of the file list happens before the area lock
//! is taken, so no lock is held across a file call.

use alloc::boxed::Box;
use alloc::vec::Vec;

struct Cursor<T> {
    entries: Vec<T>,
    pos: usize,
}

/// Whether a cursor is installed.
pub(crate) fn loaded(slot: *mut u8) -> bool {
    !slot.is_null()
}

/// Install a freshly loaded list at position zero.
pub(crate) fn store<T>(slot: &mut *mut u8, entries: Vec<T>) {
    if !slot.is_null() {
        // SAFETY: the slot holds either NULL or a pointer from `store`.
        unsafe { drop(Box::from_raw((*slot).cast::<Cursor<T>>())) };
    }
    *slot = Box::into_raw(Box::new(Cursor { entries, pos: 0 })).cast::<u8>();
}

/// The next entry, advancing the cursor; `None` at the end.
pub(crate) fn next<T: Clone>(slot: *mut u8) -> Option<T> {
    if slot.is_null() {
        return None;
    }
    // SAFETY: the slot holds a `Cursor<T>` from `store`.
    let cursor = unsafe { &mut *slot.cast::<Cursor<T>>() };
    let value = cursor.entries.get(cursor.pos).cloned();
    if value.is_some() {
        cursor.pos = cursor.pos.saturating_add(1);
    }
    value
}

/// Rewind an installed cursor; a missing cursor stays missing (the next call loads).
pub(crate) fn reset<T>(slot: *mut u8) {
    if slot.is_null() {
        return;
    }
    // SAFETY: the slot holds a `Cursor<T>` from `store`.
    unsafe { (*slot.cast::<Cursor<T>>()).pos = 0 };
}

/// Drop the cursor.
pub(crate) fn clear<T>(slot: &mut *mut u8) {
    if slot.is_null() {
        return;
    }
    // SAFETY: the slot holds a `Cursor<T>` from `store`.
    unsafe { drop(Box::from_raw((*slot).cast::<Cursor<T>>())) };
    *slot = core::ptr::null_mut();
}
