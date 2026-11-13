// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `Ok` -> value, `Err` -> `errno = e` and `-1`. This is the only module that sets `errno` on a
//! failure; a success never touches it (`donor-map/11` 3.3 rule 5).
//!
//! The full shape set of C60 4.3 item 3 is provided; the host tests exercise every function and
//! the later families of the work list use the remaining shapes.

#![allow(dead_code)]

use core::ffi::{c_int, c_long};

use raven_libc_abi::errno::Errno;

/// `Ok(v)` -> `v`; `Err(e)` -> `errno = e` and `-1`.
pub(crate) fn int(r: Result<c_int, Errno>) -> c_int {
    match r {
        Ok(value) => value,
        Err(error) => {
            super::errno::set(error);
            -1
        }
    }
}

/// Same for a `long` result.
pub(crate) fn long(r: Result<c_long, Errno>) -> c_long {
    match r {
        Ok(value) => value,
        Err(error) => {
            super::errno::set(error);
            -1
        }
    }
}

/// Same for a `size_t` result, returning `-1` as the C `(size_t)-1` on failure.
pub(crate) fn size(r: Result<usize, Errno>) -> usize {
    match r {
        Ok(value) => value,
        Err(error) => {
            super::errno::set(error);
            usize::MAX
        }
    }
}

/// Same for an `ssize_t` result.
pub(crate) fn ssize(r: Result<isize, Errno>) -> isize {
    match r {
        Ok(value) => value,
        Err(error) => {
            super::errno::set(error);
            -1
        }
    }
}

/// A pointer result; `Err` sets `errno` and returns NULL.
pub(crate) fn ptr<T>(r: Result<*mut T, Errno>) -> *mut T {
    match r {
        Ok(value) => value,
        Err(error) => {
            super::errno::set(error);
            core::ptr::null_mut()
        }
    }
}

/// A null pointer with `errno = e`.
pub(crate) fn null<T>(error: Errno) -> *mut T {
    super::errno::set(error);
    core::ptr::null_mut()
}

/// `Ok(())` -> 0; `Err(e)` -> `errno = e` and `-1`.
pub(crate) fn unit(r: Result<(), Errno>) -> c_int {
    match r {
        Ok(()) => 0,
        Err(error) => {
            super::errno::set(error);
            -1
        }
    }
}

/// The platform error type of `raven-libc-platform`: the same `-1`/`errno` contract.
pub(crate) fn platform_int(r: Result<c_int, raven_libc_platform::Errno>) -> c_int {
    match r {
        Ok(value) => value,
        Err(error) => {
            super::errno::set_platform(error);
            -1
        }
    }
}

/// A platform `Result<usize>` as the C `ssize_t` return of `read`/`write`.
pub(crate) fn platform_ssize(r: Result<usize, raven_libc_platform::Errno>) -> isize {
    match r {
        Ok(value) => match isize::try_from(value) {
            Ok(count) => count,
            Err(_) => {
                super::errno::set(Errno(75));
                -1
            }
        },
        Err(error) => {
            super::errno::set_platform(error);
            -1
        }
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
#[path = "ret_ut.rs"]
mod tests;
