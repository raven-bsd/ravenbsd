// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the backtrace export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `backtrace` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `backtrace`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn backtrace(buffer: *mut *mut core::ffi::c_void, size: c_int) -> c_int {
    if buffer.is_null() || size <= 0 {
        return 0;
    }
    let first_fp = frame_pointer();
    let count = usize::try_from(size).unwrap_or(0);
    // SAFETY: the caller gives `size` writable pointers; the frame walk reads the caller's stack.
    let frames = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<usize>(), count) };
    let mut read = |address: usize| -> Option<usize> {
        if address == 0 {
            return None;
        }
        // SAFETY: the frame-pointer chain is the caller's stack; a bad address faults exactly
        // like the C donor.
        Some(unsafe { core::ptr::read_unaligned(address as *const usize) })
    };
    let found = raven_libc_compat::execinfo::backtrace(frames, first_fp, &mut read);
    c_int::try_from(found).unwrap_or(0)
}

#[cfg(target_arch = "x86_64")]
fn frame_pointer() -> usize {
    let value: usize;
    // SAFETY: reads the frame pointer register.
    unsafe {
        core::arch::asm!("mov {}, rbp", out(reg) value, options(nomem, nostack, preserves_flags))
    };
    value
}

#[cfg(target_arch = "aarch64")]
fn frame_pointer() -> usize {
    let value: usize;
    // SAFETY: reads the frame pointer register.
    unsafe {
        core::arch::asm!("mov {}, x29", out(reg) value, options(nomem, nostack, preserves_flags))
    };
    value
}

#[cfg(target_arch = "riscv64")]
fn frame_pointer() -> usize {
    let value: usize;
    // SAFETY: reads the saved frame pointer register.
    unsafe {
        core::arch::asm!("mv {}, s0", out(reg) value, options(nomem, nostack, preserves_flags))
    };
    value
}
