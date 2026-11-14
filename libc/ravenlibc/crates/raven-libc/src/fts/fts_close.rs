// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fts_close(3)`.
use alloc::boxed::Box;

use super::{Cfts, ShellFts};

/// Close the walk and release every materialised node.
///
/// # Safety
/// `fts` is a live `FTS` returned by `fts_open`/`fts_openat`; it is not used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fts_close(fts: *mut Cfts) -> i32 {
    if fts.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    // SAFETY: the caller's FTS object and its client pointer.
    let shell = unsafe { Box::from_raw((*fts).fts_clientptr as *mut ShellFts) };
    // SAFETY: the FTS object came from `Box::into_raw` in `open_common`.
    unsafe { drop(Box::from_raw(fts)) };
    match shell.close() {
        Ok(()) => 0,
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
