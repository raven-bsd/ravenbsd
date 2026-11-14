// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `putspent`, `lckpwdf` and `ulckpwdf`.

use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicI32, Ordering};

use raven_libc_abi::fcntl::{O_CREAT, O_EXCL, O_RDWR};

use super::shadow;

static LOCK_FD: AtomicI32 = AtomicI32::new(-1);

/// `putspent(3)`.
///
/// # Safety
/// `entry` points to a valid `struct spwd`; `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putspent(entry: *const shadow::Spwd, stream: *mut c_void) -> c_int {
    if entry.is_null() || stream.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's record and FILE.
    let entry = unsafe { &*entry };
    let name = unsafe { crate::ffi::cstr::slice(entry.namp) };
    let password = unsafe { crate::ffi::cstr::slice(entry.pwdp) };
    let Ok(name) = core::str::from_utf8(name) else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    };
    let Ok(password) = core::str::from_utf8(password) else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    };
    let line = alloc::format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{}\n",
        name,
        password,
        entry.lstchg,
        entry.min,
        entry.max,
        entry.warn,
        entry.inact,
        entry.expire,
        entry.flag
    );
    // SAFETY: the caller's FILE.
    let file: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
        raven_libc_stdio::file_from_ptr(stream.cast());
    match raven_libc_stdio::fputs(file, line.as_bytes()) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// `lckpwdf(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn lckpwdf() -> c_int {
    if LOCK_FD.load(Ordering::Acquire) >= 0 {
        return -1;
    }
    let platform = crate::ffi::platform::global();
    let flags = (O_CREAT | O_EXCL | O_RDWR) as u32;
    match platform.open(b"/etc/.pwd.lock", flags, 0o600) {
        Ok(fd) => {
            LOCK_FD.store(fd, Ordering::Release);
            0
        }
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}

/// `ulckpwdf(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn ulckpwdf() -> c_int {
    let fd = LOCK_FD.swap(-1, Ordering::AcqRel);
    if fd < 0 {
        return 0;
    }
    let platform = crate::ffi::platform::global();
    let closed = platform.close(fd);
    let unlinked = platform.unlink(b"/etc/.pwd.lock");
    match (closed, unlinked) {
        (Ok(()), Ok(())) => 0,
        _ => -1,
    }
}
