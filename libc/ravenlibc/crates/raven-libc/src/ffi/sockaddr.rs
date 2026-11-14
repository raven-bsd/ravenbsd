// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `struct sockaddr *` plus `socklen_t` contract of C64 4.7: a pointer and a length become
//! a byte view, the family word is read without alignment assumptions and an in/out
//! `socklen_t *` follows the POSIX truncation rule (the caller's capacity wins, the full length
//! is what the caller should see). The socket family that consumes the helper is deferred with
//! C63; the conversion and its tests are the C64-owned part.

use core::ffi::{c_int, c_uint, c_void};

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::Sockaddr;

/// The C `struct in_addr` (one network-order word).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct InAddr {
    /// The address in network byte order.
    pub s_addr: u32,
}

/// The C `struct in6_addr` (16 bytes, 4-byte aligned).
#[repr(C, align(4))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct In6Addr {
    /// The 16 address bytes.
    pub s6_addr: [u8; 16],
}

/// A `socklen_t` in the shell's type system.
pub(crate) type SockLen = c_uint;

/// The `EFAULT` value as the shell's error type.
fn fault() -> Errno {
    Errno(EFAULT as u16)
}

/// The caller's socket address as bytes; NULL with a zero length is empty.
///
/// # Safety
/// A non-NULL `pointer` points to `len` readable bytes.
pub(crate) unsafe fn view<'a>(pointer: *const c_void, len: SockLen) -> Result<&'a [u8], Errno> {
    let len = len as usize;
    if pointer.is_null() {
        return if len == 0 { Ok(&[]) } else { Err(fault()) };
    }
    // SAFETY: the caller grants `len` readable bytes.
    Ok(unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), len) })
}

/// The caller's writable socket address as bytes.
///
/// # Safety
/// A non-NULL `pointer` points to `len` writable bytes.
pub(crate) unsafe fn view_mut<'a>(
    pointer: *mut c_void,
    len: SockLen,
) -> Result<&'a mut [u8], Errno> {
    let len = len as usize;
    if pointer.is_null() {
        return if len == 0 { Ok(&mut []) } else { Err(fault()) };
    }
    // SAFETY: the caller grants `len` writable bytes.
    Ok(unsafe { core::slice::from_raw_parts_mut(pointer.cast::<u8>(), len) })
}

/// The address family word of a byte view, or `None` when the view is shorter than `sockaddr`.
pub(crate) fn family(bytes: &[u8]) -> Option<c_int> {
    if bytes.len() < core::mem::size_of::<Sockaddr>() {
        return None;
    }
    let mut word = [0u8; 2];
    word.copy_from_slice(bytes.get(..2)?);
    Some(c_int::from(u16::from_ne_bytes(word)))
}

/// Write the in/out length: the caller's capacity, never more than the real length.
///
/// # Safety
/// `out` is NULL or points to a writable `socklen_t`.
pub(crate) unsafe fn write_len(out: *mut SockLen, full: usize, capacity: SockLen) {
    if out.is_null() {
        return;
    }
    let capped = full.min(capacity as usize);
    // SAFETY: the caller grants a writable `socklen_t`.
    unsafe { out.write(capped as SockLen) };
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "sockaddr_ut.rs"]
mod tests;
