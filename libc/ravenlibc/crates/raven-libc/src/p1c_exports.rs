// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c unistd/IO exports.
//!
//! These are thin C-ABI forwarders to the existing `raven-libc-platform` functions. The names are
//! in `Symbol.map` and the frozen list (P1f round 5); the submodules group at most four C
//! symbols per file so each codegen unit satisfies the K4 budget.

#![allow(dead_code, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_long, c_uint, c_void};

use raven_libc_abi::structs::{FdSet, Iovec, Pollfd, Rlimit, SigsetT, Statvfs, Timespec, Timeval};

use crate::ffi::platform::global;
use crate::ffi::ret;

fn map_error(error: raven_libc_platform::Errno) -> raven_libc_abi::errno::Errno {
    // Both crates use the same positive Linux/BSD error numbers (`raven-libc-platform` re-exports
    // `raven_abi::errno::Errno`); the C shell only changes the newtype.
    raven_libc_abi::errno::Errno(error.0)
}

fn unit(result: Result<(), raven_libc_platform::Errno>) -> c_int {
    ret::unit(result.map_err(map_error))
}

fn int(result: Result<c_int, raven_libc_platform::Errno>) -> c_int {
    ret::int(result.map_err(map_error))
}

fn ssize(result: Result<usize, raven_libc_platform::Errno>) -> isize {
    ret::platform_ssize(result)
}

unsafe fn fd_set<'a>(set: *mut FdSet) -> &'a mut [u64] {
    if set.is_null() {
        &mut []
    } else {
        // SAFETY: the caller supplied a writable FdSet (16 words).
        unsafe { &mut (*set).bytes }
    }
}

unsafe fn iovec_mut<'a>(iovs: *const Iovec, count: c_int) -> Vec<&'a mut [u8]> {
    let count = usize::try_from(count).unwrap_or(0);
    let mut bufs = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: the caller supplied `count` readable iovec records.
        let iov = unsafe { &*iovs.add(index) };
        // SAFETY: `iov_base`/`iov_len` obey the caller's iovec contract.
        bufs.push(unsafe {
            crate::ffi::cstr::slice_mut(iov.iov_base.cast(), iov.iov_len as usize)
        });
    }
    bufs
}

unsafe fn iovec_ref<'a>(iovs: *const Iovec, count: c_int) -> Vec<&'a [u8]> {
    let count = usize::try_from(count).unwrap_or(0);
    let mut bufs = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: the caller supplied `count` readable iovec records.
        let iov = unsafe { &*iovs.add(index) };
        // SAFETY: `iov_base`/`iov_len` obey the caller's iovec contract.
        bufs.push(unsafe { crate::ffi::cstr::bytes_n(iov.iov_base.cast(), iov.iov_len as usize) });
    }
    bufs
}

mod access;
mod dir;
mod fd;
mod file_adv;
mod mem;
mod mem_sync;
mod path;
mod poll_select;
mod proc;
mod time;
mod tty;
mod uio;

// Re-exported for the host tests in `p1c_exports_ut.rs`; the target build exports the same
// names directly from their submodules.
#[allow(unused_imports)]
pub(crate) use access::*;
#[allow(unused_imports)]
pub(crate) use dir::*;
#[allow(unused_imports)]
pub(crate) use fd::*;
#[allow(unused_imports)]
pub(crate) use file_adv::*;
#[allow(unused_imports)]
pub(crate) use mem::*;
#[allow(unused_imports)]
pub(crate) use mem_sync::*;
#[allow(unused_imports)]
pub(crate) use path::*;
#[allow(unused_imports)]
pub(crate) use poll_select::*;
#[allow(unused_imports)]
pub(crate) use proc::*;
#[allow(unused_imports)]
pub(crate) use time::*;
#[allow(unused_imports)]
pub(crate) use tty::*;
#[allow(unused_imports)]
pub(crate) use uio::*;

#[cfg(test)]
#[path = "p1c_exports_ut.rs"]
mod tests;
