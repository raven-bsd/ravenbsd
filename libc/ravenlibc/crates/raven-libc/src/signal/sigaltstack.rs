// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigaltstack(3)`: the raw kernel 0x89 stack_t (same 24-byte C layout).
use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::StackT;

/// Replace or query the alternate signal stack.
///
/// # Safety
/// `new`/`old` are NULL or point to `stack_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaltstack(new: *const StackT, old: *mut StackT) -> c_int {
    // SAFETY: the kernel copies the 24-byte C stack_t; the C and kernel layouts are identical.
    let raw = unsafe {
        raven_libc_sys::arch::syscall3(
            raven_abi::syscall::SIGALTSTACK,
            new as usize,
            old as usize,
            0,
        )
    };
    if raw < 0 {
        crate::ffi::errno::set(Errno(raw.unsigned_abs() as u16));
        return -1;
    }
    0
}
