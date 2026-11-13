// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `errno` and `h_errno` are TCB fields, never TLS variables (A-tls-5).
//!
//! Before the first thread control block exists `raven-libc-sys` keeps a static fallback cell;
//! once `init_main_thread` has installed the thread pointer, the provider answers with
//! [`errno_location`]. The exported `__errno_location` symbol is made by the shell (C60).

use crate::tcb::{Pthread, errno_location, errno_offset_from_tp};

/// The x86-64 `errno` offset from `tp` (the ABI value compilers and C54 assume).
pub const ERRNO_OFFSET_X86_64: isize = 52;

/// The offset of `errno_val` from the thread pointer on this target.
#[must_use]
pub const fn offset_from_tp() -> isize {
    errno_offset_from_tp()
}

/// The address of the current thread's `errno` cell.
#[must_use]
pub fn errno_location_for(tp: usize) -> *mut i32 {
    errno_location(tp)
}

/// The address of the current thread's `h_errno` cell.
#[must_use]
pub fn h_errno_location_for(tp: usize) -> *mut i32 {
    tp.wrapping_add_signed(h_errno_offset_from_tp()) as *mut i32
}

/// The `h_errno` offset from `tp` (a private TCB field, so no ABI value).
#[must_use]
pub const fn h_errno_offset_from_tp() -> isize {
    (core::mem::offset_of!(Pthread, h_errno_val) as isize).wrapping_sub(tp_adjust())
}

const fn tp_adjust() -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        0
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        core::mem::size_of::<Pthread>() as isize
    }
}

const _: () = {
    // The ABI constant and the structure must agree on x86-64; on variant I the offset is
    // computed from the field, so the assertion is that it is inside the TCB below `tp`.
    #[cfg(target_arch = "x86_64")]
    assert!(ERRNO_OFFSET_X86_64 == errno_offset_from_tp());
    assert!(core::mem::size_of::<i32>() == 4);
};

#[cfg(test)]
#[path = "errno_ut.rs"]
mod tests;
