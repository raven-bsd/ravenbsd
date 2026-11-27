// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `link(2)`.
use core::ffi::{c_char, c_int};

#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_libc_abi::errno::{EFAULT, ENAMETOOLONG, Errno};
use raven_libc_abi::limits::PATH_MAX;

/// Read a path argument the way the kernel's `copyinstr` does.
///
/// A pointer outside the user window is `EFAULT` (the donor `(char *)-1` case). The walk stops at
/// the NUL, at `PATH_MAX` (`ENAMETOOLONG`, as `namei`'s `copyinstr` reports) or at the end of the
/// user window (`EFAULT`), so a missing terminator can never fault the shell.
///
/// # Safety
/// `pointer` is NULL or points into the user address window.
unsafe fn path<'a>(pointer: *const c_char) -> Result<&'a [u8], Errno> {
    let start = pointer as usize as u64;
    if !(USER_MIN..=USER_END).contains(&start) {
        return Err(Errno(EFAULT as u16));
    }
    let limit = usize::try_from((USER_END - start).min(PATH_MAX as u64))
        .map_err(|_| Errno(EFAULT as u16))?;
    let mut length = 0usize;
    while length < limit {
        // SAFETY: `start + length < USER_END` by construction of `limit`.
        if unsafe { *pointer.cast::<u8>().add(length) } == 0 {
            // SAFETY: the bytes before the NUL were readable in the user window.
            return Ok(unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) });
        }
        length += 1;
    }
    if limit < PATH_MAX as usize {
        Err(Errno(EFAULT as u16))
    } else {
        Err(Errno(ENAMETOOLONG as u16))
    }
}

/// Create `newpath` as a second name for `oldpath`.
///
/// # Safety
/// Both pointers are NULL or NUL-terminated strings in the user address window.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn link(oldpath: *const c_char, newpath: *const c_char) -> c_int {
    // SAFETY: the caller's C string contract.
    let old = match unsafe { path(oldpath) } {
        Ok(value) => value,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    // SAFETY: the caller's C string contract.
    let new = match unsafe { path(newpath) } {
        Ok(value) => value,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    crate::ffi::ret::platform_int(crate::ffi::platform::global().link(old, new).map(|()| 0))
}
