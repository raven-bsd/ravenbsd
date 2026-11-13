// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the addmntent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `addmntent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::mntent_state::CMntEnt;
use alloc::vec::Vec;
use raven_libc_compat::mntent::{MntEnt, format};

/// `addmntent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn addmntent(file: *mut super::File, entry: *const CMntEnt) -> c_int {
    if file.is_null() || entry.is_null() {
        return 1;
    }
    // SAFETY: all four strings are NUL-terminated.
    let record = unsafe {
        MntEnt {
            fsname: super::c_bytes((*entry).mnt_fsname, super::MAX_NAME).to_vec(),
            dir: super::c_bytes((*entry).mnt_dir, super::MAX_NAME).to_vec(),
            type_: super::c_bytes((*entry).mnt_type, super::MAX_NAME).to_vec(),
            opts: super::c_bytes((*entry).mnt_opts, super::MAX_NAME).to_vec(),
            freq: (*entry).mnt_freq,
            passno: (*entry).mnt_passno,
        }
    };
    let mut text = Vec::new();
    format(&record, &mut text);
    // SAFETY: `file` is open and `text` is readable.
    let written = unsafe { super::fwrite(text.as_ptr().cast(), 1, text.len(), file) };
    if written == text.len() { 0 } else { 1 }
}
