// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fts_set(3)`.
use core::ffi::c_int;

use super::{Cfts, Cftsent, shell_of};

/// Apply an instruction to the node `ent`.
///
/// # Safety
/// `fts` is live; `ent` is a node returned by `fts_read`/`fts_children` for this walk.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fts_set(fts: *mut Cfts, ent: *mut Cftsent, instr: c_int) -> c_int {
    // SAFETY: the caller's FTS and node pointers.
    let Some(shell) = (unsafe { shell_of(fts) }) else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    };
    if ent.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    // SAFETY: the caller's node; `reserved[0]` carries the engine node id.
    let id = unsafe { (*ent).reserved[0] }.max(0) as usize;
    match shell.set(id, instr) {
        Ok(()) => 0,
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
