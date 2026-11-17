// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getdate(3)`: parse a string with the formats named by `DATEMSK`.

use alloc::vec;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::c_char;

use raven_libc_abi::errno::{ENOENT, Errno};
use raven_libc_abi::structs::Tm as AbiTm;
use raven_libc_time::sys::TimeSys;

use super::ShellTime;

struct StaticTm(UnsafeCell<AbiTm>);

// SAFETY: the shell is single-threaded; the C89 result object is overwritten by the next call.
unsafe impl Sync for StaticTm {}

static GETDATE_TM: StaticTm = StaticTm(UnsafeCell::new(AbiTm::ZERO));

/// `getdate`: try each DATEMSK line as a `strptime` format.
///
/// # Safety
/// `string` is a NUL-terminated input string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getdate(string: *const c_char) -> *mut AbiTm {
    if string.is_null() {
        crate::ffi::errno::set(Errno(ENOENT as u16));
        return core::ptr::null_mut();
    }
    let name = b"DATEMSK\0";
    // SAFETY: the literal is NUL-terminated.
    let path = unsafe { crate::stdlib::getenv::getenv(name.as_ptr().cast()) };
    if path.is_null() {
        crate::ffi::errno::set(Errno(ENOENT as u16));
        return core::ptr::null_mut();
    }
    let mut length = 0usize;
    // SAFETY: `getenv` returned a NUL-terminated string.
    while unsafe { *path.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the path is NUL-terminated.
    let path = unsafe { core::slice::from_raw_parts(path.cast::<u8>(), length) };
    let mut contents = vec![0u8; 65_536];
    let read = match ShellTime.read_file(path, &mut contents) {
        Ok(read) => read,
        Err(error) => {
            crate::ffi::errno::set(error);
            return core::ptr::null_mut();
        }
    };
    let data = contents.get(..read).unwrap_or(&[]);
    let mut value = AbiTm::ZERO;
    for line in data.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let mut format = Vec::with_capacity(line.len() + 1);
        format.extend_from_slice(line);
        format.push(0);
        // SAFETY: the caller's input and the temporary format are NUL-terminated.
        if !unsafe { super::strftime::strptime(string, format.as_ptr().cast(), &mut value) }
            .is_null()
        {
            // SAFETY: single-threaded static cell.
            unsafe { *GETDATE_TM.0.get() = value };
            return GETDATE_TM.0.get();
        }
    }
    crate::ffi::errno::set(Errno(ENOENT as u16));
    core::ptr::null_mut()
}
