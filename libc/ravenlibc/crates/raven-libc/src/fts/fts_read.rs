// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fts_read(3)`.
use super::{Cfts, Cftsent, shell_of};

/// Return the next node of the walk, or NULL at the end.
///
/// # Safety
/// `fts` is a live `FTS` returned by `fts_open`/`fts_openat`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fts_read(fts: *mut Cfts) -> *mut Cftsent {
    // SAFETY: the caller's FTS object.
    let Some(shell) = (unsafe { shell_of(fts) }) else {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    };
    match shell.read() {
        Ok(node) => node,
        Err(error) => {
            crate::ffi::errno::set(error);
            core::ptr::null_mut()
        }
    }
}
