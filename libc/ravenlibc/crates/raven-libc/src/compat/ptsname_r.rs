// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `ptsname_r`: the `/dev/pts/<id>` path of a master descriptor.
//!
//! `posix_openpt` records the pair id per descriptor. The pty server is a native scheme; the
//! `/dev/pts/<id>` name is mapped to `pty:slave/<id>` by the platform open path, so the public
//! name matches the donor and `ssh -tt tty` reports the expected device.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EBADF, ERANGE};

use super::posix_openpt::pair_for;

/// Write `pty:slave/<id>` plus a NUL into `buffer`.
fn slave_path(pair: u64, buffer: &mut [u8]) -> Result<usize, u16> {
    const PREFIX: &[u8] = b"/dev/pts/";
    let mut digits = [0u8; 20];
    let mut value = pair;
    let mut count = 0usize;
    loop {
        digits[count] = b'0' + u8::try_from(value % 10).map_err(|_| ERANGE as u16)?;
        count = count.saturating_add(1);
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let name = PREFIX.len().checked_add(count).ok_or(ERANGE as u16)?;
    if buffer.len() < name.saturating_add(1) {
        return Err(ERANGE as u16);
    }
    buffer
        .get_mut(..PREFIX.len())
        .ok_or(ERANGE as u16)?
        .copy_from_slice(PREFIX);
    for index in 0..count {
        match buffer.get_mut(PREFIX.len() + index) {
            Some(slot) => *slot = digits[count - 1 - index],
            None => return Err(ERANGE as u16),
        }
    }
    if let Some(slot) = buffer.get_mut(name) {
        *slot = 0;
    }
    Ok(name)
}

/// `ptsname_r`.
///
/// # Safety
/// `buffer` is NULL or writable for `length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ptsname_r(fd: c_int, buffer: *mut c_char, length: usize) -> c_int {
    if buffer.is_null() || length == 0 {
        return ERANGE as c_int;
    }
    let Some(pair) = pair_for(fd) else {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(EBADF as c_int) };
        return EBADF as c_int;
    };
    // SAFETY: the caller owns `length` writable bytes at `buffer`.
    let target = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<u8>(), length) };
    match slave_path(pair, target) {
        Ok(_) => 0,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error)) };
            c_int::from(error)
        }
    }
}

#[cfg(test)]
#[path = "ptsname_r_ut.rs"]
mod tests;
