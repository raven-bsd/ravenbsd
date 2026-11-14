// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The C59 `va_list` walk of the three variadic `exec` entries: each pointer argument becomes
//! one owned byte string, the list stops at NULL and the two C64 argument bounds apply
//! (`MAX_ARGS`, `ARG_MAX`, both `E2BIG`). `execl`/`execle`/`execlp` are the only users.
//!
//! One reader object serves both arrays of `execle`: the C59 reader copies the `va_list` and
//! never writes back (RD-new 4), so a second `reader(list)` would restart at the argv words.

use core::ffi::c_char;

use raven_libc_abi::errno::Errno;

use crate::ffi::argv::{ARG_MAX, MAX_ARGS, too_big};
use crate::va::VaReader;

/// Read the remaining arguments after the named `first` word.
pub(crate) fn collect_argv<R: VaReader>(
    values: &mut R,
    first: *const c_char,
) -> Result<alloc::vec::Vec<alloc::vec::Vec<u8>>, Errno> {
    let mut out = alloc::vec::Vec::new();
    let mut total = 0usize;
    if !first.is_null() {
        // SAFETY: the trampoline's first argument is a NUL-terminated string.
        let bytes = unsafe { crate::ffi::cstr::slice(first) };
        total = bytes.len().saturating_add(1);
        out.push(bytes.to_vec());
    }
    loop {
        let pointer = values.next_u64() as *const c_char;
        if pointer.is_null() {
            return Ok(out);
        }
        // SAFETY: each word is a NUL-terminated string.
        let bytes = unsafe { crate::ffi::cstr::slice(pointer) };
        total = total
            .checked_add(bytes.len().saturating_add(1))
            .ok_or_else(too_big)?;
        if total > ARG_MAX || out.len() >= MAX_ARGS {
            return Err(too_big());
        }
        out.push(bytes.to_vec());
    }
}

/// Read a NULL-terminated environment array from the same reader.
pub(crate) fn collect_envp<R: VaReader>(
    values: &mut R,
) -> Result<alloc::vec::Vec<alloc::vec::Vec<u8>>, Errno> {
    let mut out = alloc::vec::Vec::new();
    let mut total = 0usize;
    loop {
        let pointer = values.next_u64() as *const c_char;
        if pointer.is_null() {
            return Ok(out);
        }
        // SAFETY: each word is a NUL-terminated string.
        let bytes = unsafe { crate::ffi::cstr::slice(pointer) };
        total = total
            .checked_add(bytes.len().saturating_add(1))
            .ok_or_else(too_big)?;
        if total > ARG_MAX || out.len() >= MAX_ARGS {
            return Err(too_big());
        }
        out.push(bytes.to_vec());
    }
}
