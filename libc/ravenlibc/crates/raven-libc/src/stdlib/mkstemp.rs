// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mkstemp(3)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EEXIST, EINVAL, Errno};
use raven_libc_abi::generated::fcntl::{O_CREAT, O_EXCL, O_RDWR};
use raven_libc_stdio::traits::SysFile;

use crate::ffi::env::ShellEnv;

const MAX_TRIES: usize = 10_000;

/// `mkstemp`: replace the six trailing `X` characters and create the file exclusively.
///
/// # Safety
/// `template` is a writable NUL-terminated string with six trailing `X` characters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkstemp(template: *mut c_char) -> c_int {
    if template.is_null() {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    }
    let mut length = 0usize;
    // SAFETY: the caller's NUL-terminated string.
    while unsafe { *template.add(length) } != 0 {
        length += 1;
    }
    if length < 6 {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    }
    let start = length - 6;
    // SAFETY: `start..length` is inside the caller's string.
    let suffix = unsafe { core::slice::from_raw_parts(template.add(start).cast::<u8>(), 6) };
    if suffix.iter().any(|byte| *byte != b'X') {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    }
    let mut attempt = 0usize;
    while attempt < MAX_TRIES {
        let mut random = [0u8; 6];
        <ShellEnv as SysFile>::random_suffix(&ShellEnv, &mut random);
        // SAFETY: `start..length` is writable and inside the caller's string.
        unsafe {
            core::ptr::copy_nonoverlapping(random.as_ptr(), template.add(start).cast::<u8>(), 6);
        }
        // SAFETY: the caller's NUL-terminated path.
        let path = unsafe { crate::ffi::cstr::slice(template) };
        match crate::ffi::platform::global().open(path, (O_RDWR | O_CREAT | O_EXCL) as u32, 0o600) {
            Ok(fd) => return fd,
            Err(error) if i32::from(error.0) == EEXIST => attempt += 1,
            Err(error) => {
                crate::ffi::errno::set_platform(error);
                return -1;
            }
        }
    }
    crate::ffi::ret::int(Err(Errno(EEXIST as u16)))
}
