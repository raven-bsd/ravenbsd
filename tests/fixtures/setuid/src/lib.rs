// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw process syscalls shared by the U3b fixture binaries.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly; every block documents its contract.

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

/// `proc_self_info(out, count)`.
pub const PROC_SELF_INFO: usize = 0x35;
/// `proc_wait(proc, out_code)`.
pub const PROC_WAIT: usize = 0x31;
/// `proc_setcred_ex(which, a, b, c, groups, count)`.
pub const PROC_SETCRED_EX: usize = 0x3B;

/// Six-argument x86-64 syscall.
#[inline(always)]
pub unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

/// Read the eight words `[pid, ppid, pgid, sid, uid, euid, gid, egid]`.
pub fn self_info(out: &mut [u64; 8]) -> Result<(), Errno> {
    let raw = unsafe {
        syscall6(
            PROC_SELF_INFO,
            out.as_mut_ptr() as usize,
            out.len(),
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(raw).map(|_| ())
}

/// Change uid/euid/gid/egid with the raw `SetCred` selector.
pub fn setcred(which: usize, value: u32) -> Result<(), Errno> {
    let raw = unsafe { syscall6(PROC_SETCRED_EX, which, value as usize, 0, 0, 0, 0) };
    Errno::from_ret(raw).map(|_| ())
}

/// Wait for `process` and return its exit code.
pub fn wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    let raw = unsafe {
        syscall6(
            PROC_WAIT,
            process.0 as usize,
            &mut code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(raw).map(|_| code)
}
