// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fts_children(3)`.
use core::ffi::c_int;

use super::{Cfts, Cftsent, shell_of};

/// Return the current directory's children, linked through `fts_link`.
///
/// # Safety
/// `fts` is a live `FTS` returned by `fts_open`/`fts_openat`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fts_children(fts: *mut Cfts, instr: c_int) -> *mut Cftsent {
    // SAFETY: the caller's FTS object.
    let Some(shell) = (unsafe { shell_of(fts) }) else {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    };
    match shell.children(instr) {
        Ok(node) => node,
        Err(error) => {
            crate::ffi::errno::set(error);
            core::ptr::null_mut()
        }
    }
}
