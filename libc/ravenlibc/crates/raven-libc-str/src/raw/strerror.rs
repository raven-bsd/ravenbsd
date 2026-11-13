// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strerror_r` (XSI), `__gnu_strerror_r` and `strsignal` pointer entry points.
//!
//! RavenBSD: the XSI/GNU split of glibc `string/string.h:450-475` and doc 01 3.6 is reproduced
//! by symbol; neither form touches `errno` (A-str-7).

use crate::errmsg::{BUF_LEN, Xsi, strerror_into, strerror_r_xsi, strsignal_into};

/// XSI `strerror_r`: fill `buf` and return `0`, `EINVAL` or `ERANGE`; never sets `errno`.
///
/// # Safety
///
/// `buf` is writable for `n` bytes.
pub unsafe fn strerror_r(e: i32, buf: *mut u8, n: usize) -> i32 {
    // SAFETY: the caller provides `n` writable bytes; the engine writes only inside that slice.
    let slice = unsafe { super::common::bytes_mut(buf, n) };
    match strerror_r_xsi(e, slice) {
        Xsi::Ok => 0,
        Xsi::Einval => raven_libc_abi::errno::EINVAL,
        Xsi::Erange => raven_libc_abi::errno::ERANGE,
    }
}

/// GNU `__gnu_strerror_r`: always fills `buf`, NUL terminating a truncated message, and returns
/// `buf` (never null).
///
/// # Safety
///
/// `buf` is writable for `n` bytes.
pub unsafe fn gnu_strerror_r(e: i32, buf: *mut u8, n: usize) -> *mut u8 {
    // SAFETY: the caller provides `n` writable bytes; the copy stays inside them.
    unsafe {
        if n == 0 {
            return buf;
        }
        let mut stack = [0u8; BUF_LEN];
        let (text, _) = strerror_into(e, &mut stack);
        let usable = n.wrapping_sub(1);
        let copied = if text.len() < usable {
            text.len()
        } else {
            usable
        };
        let mut index = 0usize;
        while index < copied {
            *buf.add(index) = text.get(index).copied().unwrap_or(0);
            index = index.wrapping_add(1);
        }
        *buf.add(copied) = 0;
    }
    buf
}

/// `strsignal`: write the NUL-terminated description (or `"Unknown signal: N"`) into `buf` and
/// return `buf`.
///
/// # Safety
///
/// `buf` is writable for `n` bytes.
pub unsafe fn strsignal(s: i32, buf: *mut u8, n: usize) -> *mut u8 {
    // SAFETY: the caller provides `n` writable bytes; the copy stays inside them.
    unsafe {
        if n == 0 {
            return buf;
        }
        let mut stack = [0u8; BUF_LEN];
        let (text, _) = strsignal_into(s, &mut stack);
        let usable = n.wrapping_sub(1);
        let copied = if text.len() < usable {
            text.len()
        } else {
            usable
        };
        let mut index = 0usize;
        while index < copied {
            *buf.add(index) = text.get(index).copied().unwrap_or(0);
            index = index.wrapping_add(1);
        }
        *buf.add(copied) = 0;
    }
    buf
}
