// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! C string pointers to byte slices. The length walk is the same loop as `strlen`; `CStr` is
//! never used (C60 4.3 item 3).

use core::ffi::c_char;

#[cfg(all(any(target_os = "none", test), target_arch = "aarch64"))]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(all(any(target_os = "none", test), target_arch = "x86_64"))]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
#[cfg(any(target_os = "none", test))]
use raven_libc_abi::errno::{EFAULT, ENAMETOOLONG, Errno};
#[cfg(any(target_os = "none", test))]
use raven_libc_abi::limits::PATH_MAX;

/// A caller's NUL-terminated path, read under the kernel `copyinstr` limits.
///
/// The kernel rejects a path pointer outside the user window with `EFAULT` and a string with no
/// NUL within `PATH_MAX` with `ENAMETOOLONG`. The shell reads paths in user mode and has no
/// page-fault fixup, so it applies both predicates before it walks the bytes; an in-window path is
/// read unchanged. A window-wide pointer that is not mapped is still the caller's contract, as for
/// [`slice`].
///
/// # Safety
/// `pointer` is NULL or points into the user address window.
#[cfg(any(target_os = "none", test))]
pub(crate) unsafe fn checked_path<'a>(pointer: *const c_char) -> Result<&'a [u8], Errno> {
    if pointer.is_null() {
        return Err(Errno::from_generated(EFAULT));
    }
    let start = pointer as usize as u64;
    if !(USER_MIN..USER_END).contains(&start) {
        return Err(Errno::from_generated(EFAULT));
    }
    let mut length = 0usize;
    while length < PATH_MAX as usize {
        let address = start.saturating_add(length as u64);
        if address >= USER_END {
            return Err(Errno::from_generated(EFAULT));
        }
        // SAFETY: `address` is inside the user window and the caller guarantees the mapping.
        if unsafe { *pointer.cast::<u8>().add(length) } == 0 {
            // SAFETY: `length` readable bytes precede the NUL the walk found.
            return Ok(unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) });
        }
        length = length.saturating_add(1);
    }
    Err(Errno::from_generated(ENAMETOOLONG))
}

/// A NUL-terminated C string as a read-only slice.
///
/// # Safety
/// `pointer` is NULL or points to a readable NUL-terminated string.
pub(crate) unsafe fn slice<'a>(pointer: *const c_char) -> &'a [u8] {
    if pointer.is_null() {
        return &[];
    }
    // SAFETY: the caller's contract is the C string contract; the walk stops at the NUL.
    let length = unsafe { length(pointer) };
    // SAFETY: `length` readable bytes precede the NUL the walk found.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) }
}

/// The first `n` bytes, or up to the first NUL.
///
/// # Safety
/// `pointer` is NULL or points to at least `n` readable bytes, or to a NUL before them.
pub(crate) unsafe fn slice_n<'a>(pointer: *const c_char, n: usize) -> &'a [u8] {
    if pointer.is_null() {
        return &[];
    }
    let mut length = 0usize;
    while length < n {
        // SAFETY: the caller guarantees `n` readable bytes or a NUL inside them.
        if unsafe { *pointer.cast::<u8>().add(length) } == 0 {
            break;
        }
        length = length.wrapping_add(1);
    }
    // SAFETY: `length <= n` bytes were read.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) }
}

/// The first `n` bytes of a binary buffer, regardless of its contents.
///
/// # Safety
/// `pointer` is NULL or points to at least `n` readable bytes.
pub(crate) unsafe fn bytes_n<'a>(pointer: *const c_char, n: usize) -> &'a [u8] {
    if pointer.is_null() || n == 0 {
        return &[];
    }
    // SAFETY: the caller promises `n` readable bytes; embedded NULs are data.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), n) }
}

/// A caller's writable buffer.
///
/// # Safety
/// `pointer` is non-NULL and points to `n` writable bytes.
pub(crate) unsafe fn slice_mut<'a>(pointer: *mut c_char, n: usize) -> &'a mut [u8] {
    if pointer.is_null() || n == 0 {
        // A zero-length region is empty even at a NULL pointer; never `from_raw_parts_mut(null, 0)`.
        return &mut [];
    }
    // SAFETY: the caller promises `n` writable bytes.
    unsafe { core::slice::from_raw_parts_mut(pointer.cast::<u8>(), n) }
}

/// The byte length of a NUL-terminated string.
///
/// # Safety
/// `pointer` points to a readable NUL-terminated string.
unsafe fn length(pointer: *const c_char) -> usize {
    let mut length = 0usize;
    loop {
        // SAFETY: the caller's string has a NUL; the walk cannot pass it.
        if unsafe { *pointer.add(length) } == 0 {
            return length;
        }
        length = length.wrapping_add(1);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "cstr_ut.rs"]
mod tests;
