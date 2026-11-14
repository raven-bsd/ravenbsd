// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! A NULL-terminated `char *const[]` as a bounded view (C64 4.6): the walk counts the words and
//! the string bytes without allocating, rejects a missing terminator beyond `MAX_ARGS`, more
//! than `ARG_MAX` bytes in total (both `E2BIG`) and hands each string to a callback as a slice.
//! The process crate's [`raven_libc_proc::limits`] is the one source of the two bounds.

use core::ffi::c_char;

use raven_libc_abi::errno::{E2BIG, Errno};

/// The largest accepted argument pointer count.
pub(crate) const MAX_ARGS: usize = raven_libc_proc::limits::MAX_ARGS;
/// The largest accepted argument plus environment byte count.
pub(crate) const ARG_MAX: usize = raven_libc_proc::limits::ARG_MAX;

/// The `E2BIG` value as the shell's error type.
pub(crate) fn too_big() -> Errno {
    Errno(E2BIG as u16)
}

/// Count the argument words and their bytes, without reading a string twice.
///
/// # Safety
/// `argv` is NULL or a NULL-terminated array of NUL-terminated strings.
pub(crate) unsafe fn scan(argv: *const *const c_char) -> Result<(usize, usize), Errno> {
    if argv.is_null() {
        return Ok((0, 0));
    }
    let mut count = 0usize;
    let mut total = 0usize;
    loop {
        // SAFETY: the caller's NULL-terminated array; a word at `MAX_ARGS` is read only to
        // distinguish a terminated array (the bound itself is legal) from an unterminated one.
        let word = unsafe { *argv.add(count) };
        if word.is_null() {
            return Ok((count, total));
        }
        if count >= MAX_ARGS {
            return Err(too_big());
        }
        // SAFETY: a non-NULL word names a NUL-terminated string.
        let length = unsafe { crate::ffi::cstr::slice(word) }.len();
        total = total
            .checked_add(length.saturating_add(1))
            .ok_or_else(too_big)?;
        if total > ARG_MAX {
            return Err(too_big());
        }
        count = count.saturating_add(1);
    }
}

/// Call `visit` once per argument string.
///
/// # Safety
/// As [`scan`]; additionally `visit` must not retain its argument past the call.
pub(crate) unsafe fn walk<F: FnMut(&[u8]) -> Result<(), Errno>>(
    argv: *const *const c_char,
    visit: &mut F,
) -> Result<(), Errno> {
    if argv.is_null() {
        return Ok(());
    }
    let mut index = 0usize;
    loop {
        // SAFETY: as in `scan`; the walk stops at the first NULL word.
        let word = unsafe { *argv.add(index) };
        if word.is_null() {
            return Ok(());
        }
        if index >= MAX_ARGS {
            return Err(too_big());
        }
        // SAFETY: a non-NULL word names a NUL-terminated string.
        visit(unsafe { crate::ffi::cstr::slice(word) })?;
        index = index.saturating_add(1);
    }
}

/// Copy every argument into an owned vector, enforcing the two bounds.
///
/// # Safety
/// As [`scan`].
pub(crate) unsafe fn collect(
    argv: *const *const c_char,
) -> Result<alloc::vec::Vec<alloc::vec::Vec<u8>>, Errno> {
    let mut out = alloc::vec::Vec::new();
    // SAFETY: the caller's array contract; `walk` enforces both bounds.
    unsafe {
        walk(argv, &mut |argument| {
            out.push(argument.to_vec());
            Ok(())
        })?;
    }
    Ok(out)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "argv_ut.rs"]
mod tests;
