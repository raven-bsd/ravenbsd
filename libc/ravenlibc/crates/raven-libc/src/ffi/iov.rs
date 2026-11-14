// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `struct iovec[]` as bounded byte-slice views (C64 4.7). A negative count, a total above
//! `isize::MAX` and a NULL base with a non-zero length are `EINVAL`; the conversion never
//! copies. The socket family that consumes it is deferred with C63, the helper and its tests
//! are the C64-owned part.

use core::ffi::c_int;

use raven_libc_abi::errno::{EINVAL, Errno};
use raven_libc_abi::structs::Iovec;

/// The `EINVAL` value as the shell's error type.
fn invalid() -> Errno {
    Errno(EINVAL as u16)
}

/// The byte total of `iovs`, rejecting `isize::MAX` and beyond.
pub(crate) fn total(iovs: &[Iovec]) -> Result<usize, Errno> {
    let mut total = 0usize;
    for iovec in iovs {
        let length = usize::try_from(iovec.iov_len).map_err(|_| invalid())?;
        total = total.checked_add(length).ok_or_else(invalid)?;
        if total > isize::MAX as usize {
            return Err(invalid());
        }
    }
    Ok(total)
}

/// Read-only views over the caller's `iovec` array.
///
/// # Safety
/// `iov` is NULL with a zero count, or points to `count` initialised entries whose bases are
/// readable for their lengths.
pub(crate) unsafe fn slices<'a>(
    iov: *const Iovec,
    count: c_int,
) -> Result<alloc::vec::Vec<&'a [u8]>, Errno> {
    // SAFETY: the caller's `iovec` array contract.
    let entries = unsafe { entries(iov, count) }?;
    let mut out = alloc::vec::Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.iov_base.is_null() && entry.iov_len != 0 {
            return Err(invalid());
        }
        // SAFETY: the caller grants `iov_len` readable bytes at the base.
        out.push(unsafe {
            crate::ffi::cast::bytes(entry.iov_base.cast_const(), entry.iov_len as usize)
        });
    }
    Ok(out)
}

/// Mutable views over the caller's `iovec` array.
///
/// # Safety
/// As [`slices`], with writable bases.
pub(crate) unsafe fn slices_mut<'a>(
    iov: *const Iovec,
    count: c_int,
) -> Result<alloc::vec::Vec<&'a mut [u8]>, Errno> {
    // SAFETY: the caller's `iovec` array contract.
    let entries = unsafe { entries(iov, count) }?;
    let mut out = alloc::vec::Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.iov_base.is_null() && entry.iov_len != 0 {
            return Err(invalid());
        }
        // SAFETY: the caller grants `iov_len` writable bytes at the base, and no two entries
        // overlap (the C `readv` contract).
        out.push(unsafe { crate::ffi::cast::bytes_mut(entry.iov_base, entry.iov_len as usize) });
    }
    Ok(out)
}

/// The validated entry slice behind the two conversions.
///
/// # Safety
/// As [`slices`].
unsafe fn entries<'a>(iov: *const Iovec, count: c_int) -> Result<&'a [Iovec], Errno> {
    if count < 0 {
        return Err(invalid());
    }
    if iov.is_null() {
        return if count == 0 { Ok(&[]) } else { Err(invalid()) };
    }
    let count = usize::try_from(count).map_err(|_| invalid())?;
    // SAFETY: the caller grants `count` initialised entries.
    let entries = unsafe { core::slice::from_raw_parts(iov, count) };
    total(entries)?;
    Ok(entries)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "iov_ut.rs"]
mod tests;
