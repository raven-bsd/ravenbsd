// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `putgrent` from `<grp.h>` over C51's formatter.

use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Group as CGroup;
use raven_libc_pwd::{ByteSink, Group};

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

/// Read the NULL-terminated `gr_mem` array.
///
/// # Safety
/// `pointer` is NULL or a NULL-terminated array of C strings.
unsafe fn members(pointer: *mut c_void) -> Vec<String> {
    let mut out = Vec::new();
    if pointer.is_null() {
        return out;
    }
    let mut index = 0usize;
    loop {
        // SAFETY: the caller's NULL-terminated array.
        let slot = unsafe { *(pointer.cast::<*mut c_char>()).add(index) };
        if slot.is_null() {
            break;
        }
        // SAFETY: the slot is a NUL-terminated member name.
        out.push(String::from_utf8_lossy(unsafe { crate::ffi::cstr::slice(slot) }).into_owned());
        index = index.saturating_add(1);
        if index > 65_536 {
            break;
        }
    }
    out
}

/// `putgrent`.
///
/// # Safety
/// `entry` is a valid `struct group`; `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putgrent(entry: *const CGroup, stream: *mut c_void) -> c_int {
    if entry.is_null() || stream.is_null() {
        return crate::ffi::ret::int(Err(Errno(14)));
    }
    // SAFETY: the caller's record.
    let record = unsafe { &*entry };
    let value = Group {
        // SAFETY: the record's fields are the caller's C strings.
        name: unsafe { field(record.gr_name) },
        passwd: unsafe { field(record.gr_passwd) },
        gid: record.gr_gid,
        mem: unsafe { members(record.gr_mem) },
    };
    let mut sink = FileSink(stream);
    match raven_libc_pwd::putgrent(&value, &mut sink) {
        Ok(()) => 0,
        Err(code) => crate::ffi::ret::int(Err(Errno(u16::try_from(code).unwrap_or(5)))),
    }
}
