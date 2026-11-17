// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `ftok`: the POSIX key derivation over `stat(2)` data. This one is implemented for real.

use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;
use raven_libc_abi::types::key_t;
use raven_libc_ipc::sysv;

use super::fail;

unsafe extern "C" {
    fn stat(path: *const c_char, buffer: *mut Stat) -> c_int;
}

/// `ftok`.
///
/// # Safety
/// `path` is a NUL-terminated path name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftok(path: *const c_char, id: c_int) -> key_t {
    if path.is_null() {
        let _ = fail(raven_libc_ipc::EINVAL);
        return -1;
    }
    let mut info = Stat::ZERO;
    // SAFETY: `path` is a NUL-terminated string and `info` is writable; `stat` sets errno on
    // failure.
    let status = unsafe { stat(path, &mut info) };
    if status != 0 {
        return -1;
    }
    sysv::ftok_key(info.st_dev, info.st_ino, (id & 0xff) as u8)
}
