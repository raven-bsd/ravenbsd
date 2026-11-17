// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tmpfile`, `tmpfile64`, `tmpnam` and `cuserid`.

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_void};
use core::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_stdio::{Fail, file_from_ptr};

use crate::stdio::streams::streams;

struct StaticName(UnsafeCell<[u8; 32]>);

// SAFETY: the shell is single-threaded; the static name is overwritten by the next call.
unsafe impl Sync for StaticName {}

static TMPNAME: StaticName = StaticName(UnsafeCell::new([0; 32]));
static NAME_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn random_name(out: &mut [u8]) {
    let value = NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut state = value ^ 0x9e37_79b9;
    for slot in out.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let index = state % 36;
        *slot = *b"abcdefghijklmnopqrstuvwxyz0123456789"
            .get(index)
            .unwrap_or(&b'0');
    }
}

/// `tmpfile`: create an anonymous stream.
#[unsafe(no_mangle)]
pub extern "C" fn tmpfile() -> *mut c_void {
    match raven_libc_stdio::tmpfile(streams().state()) {
        Ok(file) => file.cast(),
        Err(error) => {
            if let Fail::Errno(value) = error {
                crate::ffi::errno::set(value);
            }
            core::ptr::null_mut()
        }
    }
}

/// `tmpfile64`: the 64-bit alias of `tmpfile`.
#[unsafe(no_mangle)]
pub extern "C" fn tmpfile64() -> *mut c_void {
    tmpfile()
}

/// `tmpnam`: a unique path in the C89 static buffer.
///
/// # Safety
/// `buffer` is NULL or writable for `L_tmpnam` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tmpnam(buffer: *mut c_char) -> *mut c_char {
    let target = if buffer.is_null() {
        TMPNAME.0.get().cast::<c_char>()
    } else {
        buffer
    };
    let prefix = b"/tmp/rl";
    // SAFETY: the caller grants `L_tmpnam` writable bytes (or the static cell is used).
    unsafe {
        core::ptr::copy_nonoverlapping(prefix.as_ptr(), target.cast::<u8>(), prefix.len());
        let mut suffix = [0u8; 6];
        random_name(&mut suffix);
        core::ptr::copy_nonoverlapping(suffix.as_ptr(), target.cast::<u8>().add(prefix.len()), 6);
        *target.cast::<u8>().add(prefix.len() + 6) = 0;
    }
    target
}

/// `cuserid`: the effective user name, or NULL when no `LOGNAME` is set.
///
/// # Safety
/// `buffer` is NULL or writable for `L_cuserid` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cuserid(buffer: *mut c_char) -> *mut c_char {
    let name = b"LOGNAME\0";
    // SAFETY: the literal is NUL terminated.
    let value = unsafe { crate::stdlib::getenv::getenv(name.as_ptr().cast()) };
    if value.is_null() {
        return core::ptr::null_mut();
    }
    let mut length = 0usize;
    // SAFETY: `getenv` returned a NUL-terminated string.
    while unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    let target = if buffer.is_null() {
        TMPNAME.0.get().cast::<c_char>()
    } else {
        buffer
    };
    // SAFETY: the caller grants `L_cuserid` writable bytes (or the static cell is used).
    unsafe {
        core::ptr::copy_nonoverlapping(value, target, length);
        *target.add(length) = 0;
    }
    target
}
