// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `struct sockaddr *` plus `socklen_t` contract: a pointer and a length become a byte
//! view, the FreeBSD `sa_len`/`sa_family` prefix is read without alignment assumptions, and an
//! in/out `socklen_t *` reports the full required length even when the caller's buffer is
//! smaller (FreeBSD `user_getsockname`).

use core::ffi::{c_int, c_uint, c_void};

use raven_libc_abi::errno::{EFAULT, Errno};

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

/// The caller's optional control buffer; NULL with a non-zero length is `EFAULT`.
///
/// # Safety
/// A non-NULL `pointer` points to `len` readable bytes.
pub(crate) unsafe fn control<'a>(pointer: *const c_void, len: SockLen) -> Result<&'a [u8], Errno> {
    let len = len as usize;
    if pointer.is_null() {
        return if len == 0 { Ok(&[]) } else { Err(fault()) };
    }
    // SAFETY: the caller grants `len` readable bytes.
    Ok(unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), len) })
}

/// The caller's writable optional control buffer; NULL with a non-zero length is `EFAULT`.
///
/// # Safety
/// A non-NULL `pointer` points to `len` writable bytes.
pub(crate) unsafe fn control_mut<'a>(
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

/// The address family byte of a byte view.
///
/// FreeBSD `struct sockaddr` starts with `sa_len` and puts `sa_family` at offset 1, so a
/// view shorter than two bytes has no family. Reading a two-byte word at offset 0 is the
/// S174 defect this cutover removes.
pub(crate) fn family(bytes: &[u8]) -> Option<c_int> {
    if bytes.len() < 2 {
        return None;
    }
    Some(c_int::from(*bytes.get(1)?))
}

/// Write the in/out length.
///
/// FreeBSD `user_getsockname` copies only `min(ss_len, caller_len)` bytes out, but stores the
/// full `ss_len` in `*alen` so the caller learns the size it needs. The write itself is already
/// bounded by the caller-provided `view_mut` capacity.
///
/// # Safety
/// `out` is NULL or points to a writable `socklen_t`.
pub(crate) unsafe fn write_len(out: *mut SockLen, full: usize, _capacity: SockLen) {
    if out.is_null() {
        return;
    }
    // SAFETY: the caller grants a writable `socklen_t`.
    unsafe { out.write(full as SockLen) };
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
