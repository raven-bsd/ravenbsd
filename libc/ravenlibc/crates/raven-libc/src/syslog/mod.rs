// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<syslog.h>` exports.
//!
//! The base has no `/dev/log` daemon, so `vsyslog` formats the message with the shared
//! `vsnprintf` engine and writes it to standard error. That is honest local logging, not a
//! fabricated remote or daemon answer. The ident, facility and mask are process state.

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int};
use core::sync::atomic::{AtomicI32, Ordering};

use raven_libc_abi::errno::Errno;

use crate::stdio::va::List;

mod trampolines;
pub(crate) mod vsyslog;

struct IdentCell(UnsafeCell<alloc::vec::Vec<u8>>);

// SAFETY: the shell is single-threaded until C57; `openlog`/`vsyslog` are one call at a time.
unsafe impl Sync for IdentCell {}

static IDENT: IdentCell = IdentCell(UnsafeCell::new(alloc::vec::Vec::new()));
static OPTION: AtomicI32 = AtomicI32::new(0);
static FACILITY: AtomicI32 = AtomicI32::new(1 << 3);
static MASK: AtomicI32 = AtomicI32::new(0xff);

/// The current ident bytes.
pub(crate) fn ident() -> &'static [u8] {
    // SAFETY: single-threaded access; the cell outlives the borrow.
    unsafe { &*IDENT.0.get() }
}

/// The current facility.
pub(crate) fn facility() -> c_int {
    FACILITY.load(Ordering::Relaxed)
}

/// Whether a priority is enabled by the current mask.
pub(crate) fn enabled(priority: c_int) -> bool {
    let mask = MASK.load(Ordering::Relaxed);
    let severity = priority & 7;
    mask & (1 << severity) != 0
}

/// `openlog`.
///
/// # Safety
/// `ident` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openlog(
    ident_ptr: *const c_char,
    option_value: c_int,
    facility_value: c_int,
) {
    if !ident_ptr.is_null() {
        // SAFETY: the caller's C string contract.
        let bytes = unsafe { crate::ffi::cstr::slice(ident_ptr) };
        // SAFETY: single-threaded access.
        let cell = unsafe { &mut *IDENT.0.get() };
        cell.clear();
        cell.extend_from_slice(bytes);
    }
    OPTION.store(option_value, Ordering::Relaxed);
    FACILITY.store(facility_value, Ordering::Relaxed);
}

/// `closelog`.
#[unsafe(no_mangle)]
pub extern "C" fn closelog() {
    // SAFETY: single-threaded access.
    let cell = unsafe { &mut *IDENT.0.get() };
    cell.clear();
    OPTION.store(0, Ordering::Relaxed);
}

/// `setlogmask`.
#[unsafe(no_mangle)]
pub extern "C" fn setlogmask(mask: c_int) -> c_int {
    if mask == 0 {
        return MASK.load(Ordering::Relaxed);
    }
    MASK.swap(mask, Ordering::Relaxed)
}

/// Format one message and write it to standard error.
///
/// # Safety
/// `format` is NUL terminated; `list` is the trampoline's `va_list`.
pub(crate) unsafe fn emit(priority: c_int, format: *const c_char, list: *mut List) {
    if format.is_null() {
        crate::ffi::errno::set(Errno(22));
        return;
    }
    let mut line = [0u8; 512];
    // SAFETY: the caller's format contract and trap.
    let written = unsafe {
        crate::stdio::vsnprintf::vsnprintf(
            line.as_mut_ptr().cast::<c_char>(),
            line.len(),
            format,
            list,
        )
    };
    if written < 0 {
        return;
    }
    let length = usize::try_from(written)
        .unwrap_or(0)
        .min(line.len().saturating_sub(1));
    let message = line.get(..length).unwrap_or(&[]);

    let mut out = [0u8; 768];
    let mut position = 0usize;
    let pushed = |out: &mut [u8], position: &mut usize, bytes: &[u8]| -> bool {
        let Some(end) = position.checked_add(bytes.len()) else {
            return false;
        };
        let Some(slot) = out.get_mut(*position..end) else {
            return false;
        };
        slot.copy_from_slice(bytes);
        *position = end;
        true
    };

    let facility_value = facility();
    let priority_value = priority | facility_value;
    let mut digits = [0u8; 20];
    let mut count = 0usize;
    let mut value = priority_value;
    loop {
        let Some(slot) = digits.get_mut(count) else {
            return;
        };
        *slot = b'0'.wrapping_add((value % 10) as u8);
        count = count.saturating_add(1);
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let mut ok = pushed(&mut out, &mut position, b"<");
    for index in (0..count).rev() {
        if let Some(digit) = digits.get(index).copied() {
            ok = ok && pushed(&mut out, &mut position, &[digit]);
        }
    }
    ok = ok && pushed(&mut out, &mut position, b">");
    let ident = ident();
    if !ident.is_empty() {
        ok = ok && pushed(&mut out, &mut position, ident) && pushed(&mut out, &mut position, b": ");
    }
    ok = ok && pushed(&mut out, &mut position, message) && pushed(&mut out, &mut position, b"\n");
    if !ok {
        crate::ffi::errno::set(Errno(7));
        return;
    }
    if enabled(priority) {
        let _ = crate::ffi::platform::global().write(2, out.get(..position).unwrap_or(&[]));
    }
}
