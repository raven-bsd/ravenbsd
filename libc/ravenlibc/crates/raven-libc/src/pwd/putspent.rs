// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `putspent`, `lckpwdf` and `ulckpwdf`.

use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicI32, Ordering};

use raven_libc_abi::fcntl::{O_CREAT, O_EXCL, O_RDWR};

use super::shadow;

static LOCK_FD: AtomicI32 = AtomicI32::new(-1);

/// Append `bytes` to `out` at `position`; false when the buffer is full.
fn push(out: &mut [u8], position: &mut usize, bytes: &[u8]) -> bool {
    let Some(end) = position.checked_add(bytes.len()) else {
        return false;
    };
    let Some(slot) = out.get_mut(*position..end) else {
        return false;
    };
    slot.copy_from_slice(bytes);
    *position = end;
    true
}

/// Append the unsigned decimal form of `value`.
fn push_unsigned(out: &mut [u8], position: &mut usize, value: u64) -> bool {
    let mut digits = [0u8; 20];
    let mut count = 0usize;
    let mut rest = value;
    loop {
        let Some(slot) = digits.get_mut(count) else {
            return false;
        };
        *slot = b'0'.wrapping_add((rest.wrapping_rem(10)) as u8);
        count = count.saturating_add(1);
        rest = rest.wrapping_div(10);
        if rest == 0 {
            break;
        }
    }
    for index in (0..count).rev() {
        let Some(digit) = digits.get(index).copied() else {
            return false;
        };
        if !push(out, position, &[digit]) {
            return false;
        }
    }
    true
}

/// Append the signed decimal form of `value`.
fn push_signed(out: &mut [u8], position: &mut usize, value: i64) -> bool {
    if value < 0 && !push(out, position, b"-") {
        return false;
    }
    push_unsigned(out, position, value.unsigned_abs())
}

/// Render one shadow line into `out`; `None` when it does not fit.
fn render_line(
    entry: &shadow::Spwd,
    name: &[u8],
    password: &[u8],
    out: &mut [u8],
) -> Option<usize> {
    let mut position = 0usize;
    let mut ok = push(out, &mut position, name)
        && push(out, &mut position, b":")
        && push(out, &mut position, password)
        && push(out, &mut position, b":")
        && push_signed(out, &mut position, entry.lstchg as i64)
        && push(out, &mut position, b":")
        && push_signed(out, &mut position, entry.min as i64)
        && push(out, &mut position, b":")
        && push_signed(out, &mut position, entry.max as i64)
        && push(out, &mut position, b":")
        && push_signed(out, &mut position, entry.warn as i64)
        && push(out, &mut position, b":")
        && push_signed(out, &mut position, entry.inact as i64)
        && push(out, &mut position, b":")
        && push_signed(out, &mut position, entry.expire as i64)
        && push(out, &mut position, b":")
        && push_unsigned(out, &mut position, entry.flag as u64)
        && push(out, &mut position, b"\n");
    if !ok {
        position = 0;
    }
    ok.then_some(position)
}

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
    let mut line = [0u8; 1024];
    let Some(length) = render_line(entry, name.as_bytes(), password.as_bytes(), &mut line) else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ERANGE as u16,
        )));
    };
    // SAFETY: the caller's FILE.
    let file: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
        raven_libc_stdio::file_from_ptr(stream.cast());
    match raven_libc_stdio::fputs(file, line.get(..length).unwrap_or(&[])) {
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

#[cfg(test)]
#[path = "putspent_ut.rs"]
mod tests;
