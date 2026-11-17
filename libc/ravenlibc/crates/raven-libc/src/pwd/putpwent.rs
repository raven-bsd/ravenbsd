// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `putpwent` from `<pwd.h>` over C51's formatter.

use alloc::string::String;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Passwd as CPasswd;
use raven_libc_pwd::{ByteSink, Passwd};

/// A `FILE` byte sink.
struct FileSink(*mut c_void);

impl ByteSink for FileSink {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), i32> {
        // SAFETY: the caller's FILE pointer.
        let file: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
            raven_libc_stdio::file_from_ptr(self.0.cast());
        raven_libc_stdio::fputs(file, bytes).map_err(|_| 5)
    }
}

/// A C field as a string; a NULL field is the empty string.
///
/// # Safety
/// `pointer` is NULL or NUL terminated.
unsafe fn field(pointer: *mut c_void) -> String {
    // SAFETY: the caller's C string contract.
    String::from_utf8_lossy(unsafe { crate::ffi::cstr::slice(pointer.cast::<c_char>()) })
        .into_owned()
}

/// `putpwent`.
///
/// # Safety
/// `entry` is a valid `struct passwd`; `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putpwent(entry: *const CPasswd, stream: *mut c_void) -> c_int {
    if entry.is_null() || stream.is_null() {
        return crate::ffi::ret::int(Err(Errno(14)));
    }
    // SAFETY: the caller's record.
    let record = unsafe { &*entry };
    let value = Passwd {
        // SAFETY: the record's fields are the caller's C strings.
        name: unsafe { field(record.pw_name) },
        passwd: unsafe { field(record.pw_passwd) },
        uid: record.pw_uid,
        gid: record.pw_gid,
        gecos: unsafe { field(record.pw_gecos) },
        dir: unsafe { field(record.pw_dir) },
        shell: unsafe { field(record.pw_shell) },
    };
    let mut sink = FileSink(stream);
    match raven_libc_pwd::putpwent(&value, &mut sink) {
        Ok(()) => 0,
        Err(code) => crate::ffi::ret::int(Err(Errno(u16::try_from(code).unwrap_or(5)))),
    }
}
