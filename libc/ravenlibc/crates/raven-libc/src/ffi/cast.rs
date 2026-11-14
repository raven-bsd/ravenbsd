// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one pointer-to-object helper of the shell (C64 4.1): opaque C blobs become typed views,
//! optional pointers become `Option<&T>` and caller buffers become byte slices. Every function
//! is unsafe and carries its contract in the export's `# Safety` section.

/// A mutable view of the object `pointer` names.
///
/// # Safety
/// `pointer` points to an initialised object of type `T` that outlives `'a`.
pub(crate) unsafe fn obj<'a, T>(pointer: *mut T) -> &'a mut T {
    // SAFETY: the caller's contract: a valid, exclusively borrowed object.
    unsafe { &mut *pointer }
}

/// A shared view of the object `pointer` names.
///
/// # Safety
/// `pointer` points to an initialised object of type `T` that outlives `'a`.
pub(crate) unsafe fn obj_ref<'a, T>(pointer: *const T) -> &'a T {
    // SAFETY: the caller's contract: a valid object.
    unsafe { &*pointer }
}

/// `pointer` as an optional shared view; NULL is `None`.
///
/// # Safety
/// A non-NULL `pointer` points to an initialised `T` that outlives `'a`.
pub(crate) unsafe fn opt<'a, T>(pointer: *const T) -> Option<&'a T> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the caller's contract for a non-NULL pointer.
        Some(unsafe { &*pointer })
    }
}

/// `pointer` as an optional mutable view; NULL is `None`.
///
/// # Safety
/// A non-NULL `pointer` points to an initialised `T` that outlives `'a` and is not shared.
pub(crate) unsafe fn opt_mut<'a, T>(pointer: *mut T) -> Option<&'a mut T> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the caller's contract for a non-NULL pointer.
        Some(unsafe { &mut *pointer })
    }
}

/// A caller buffer as a read-only byte slice; NULL with zero is empty.
///
/// # Safety
/// A non-NULL `pointer` points to at least `len` readable bytes.
pub(crate) unsafe fn bytes<'a>(pointer: *const core::ffi::c_void, len: usize) -> &'a [u8] {
    if pointer.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: the caller grants `len` readable bytes.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), len) }
}

/// A caller buffer as a mutable byte slice; NULL with zero is empty.
///
/// # Safety
/// A non-NULL `pointer` points to at least `len` writable bytes.
pub(crate) unsafe fn bytes_mut<'a>(pointer: *mut core::ffi::c_void, len: usize) -> &'a mut [u8] {
    if pointer.is_null() || len == 0 {
        return &mut [];
    }
    // SAFETY: the caller grants `len` writable bytes.
    unsafe { core::slice::from_raw_parts_mut(pointer.cast::<u8>(), len) }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "cast_ut.rs"]
mod tests;
