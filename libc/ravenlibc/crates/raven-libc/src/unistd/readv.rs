// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `readv(2)`: scatter a read over the caller's iovec array.

use alloc::vec::Vec;

#[cfg(target_os = "none")]
use core::ffi::c_int;
#[cfg(target_os = "none")]
use raven_libc_abi::limits::IOV_MAX;
#[cfg(target_os = "none")]
use raven_libc_abi::structs::Iovec;
use raven_libc_platform::Errno;

/// Read into the iovecs in order and return the byte count.
///
/// A short read stops the walk; an error after some bytes returns the bytes read,
/// matching the POSIX `readv` contract. The helper is separated from the C entry
/// so the short-read and error cases are host-testable.
pub(crate) fn readv_impl<F>(slices: Vec<&mut [u8]>, mut read: F) -> Result<usize, Errno>
where
    F: FnMut(&mut [u8]) -> Result<usize, Errno>,
{
    let mut total = 0usize;
    for slice in slices {
        if slice.is_empty() {
            continue;
        }
        match read(slice) {
            Ok(0) => break,
            Ok(count) => {
                let count = count.min(slice.len());
                total = total.checked_add(count).ok_or(Errno::EOVERFLOW)?;
                if count < slice.len() {
                    break;
                }
            }
            Err(error) => {
                if total == 0 {
                    return Err(error);
                }
                break;
            }
        }
    }
    Ok(total)
}

/// `readv(fd, iov, iovcnt)`.
///
/// # Safety
/// `iov` is NULL with a zero count, or points to `count` initialised entries whose
/// bases are writable for their lengths and do not overlap.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readv(fd: c_int, iov: *const Iovec, count: c_int) -> isize {
    if !(0..=IOV_MAX).contains(&count) {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
        return -1;
    }
    // SAFETY: the caller's iovec contract.
    let slices = match unsafe { crate::ffi::iov::slices_mut(iov, count) } {
        Ok(slices) => slices,
        Err(error) => {
            crate::ffi::errno::set(error);
            return -1;
        }
    };
    let platform = crate::ffi::platform::global();
    crate::ffi::ret::platform_ssize(readv_impl(slices, |buffer| platform.read(fd, buffer)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "readv_ut.rs"]
mod tests;
