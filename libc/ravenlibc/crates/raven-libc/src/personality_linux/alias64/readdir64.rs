// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `readdir64`: the 64-bit `ino_t` spelling of `readdir` (ticket C66 5.F).
use raven_libc_abi::structs::Dirent;

use crate::fts::ShellDir;

/// Forward to `readdir`.
///
/// # Safety
/// `dir` is a live `DIR` from `opendir`/`fdopendir`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readdir64(dir: *mut ShellDir) -> *mut Dirent {
    // SAFETY: forwarded from this function's contract.
    unsafe { crate::fts::readdir::readdir(dir) }
}
