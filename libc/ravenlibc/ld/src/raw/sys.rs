// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw syscalls, mirrored from `userland/rt/src/sys.rs` (no dependency on `raven-abi`).

use core::arch::asm;

/// `EXIT`.
pub const EXIT: usize = 0x01;
/// `DEBUG_WRITE`.
pub const DEBUG_WRITE: usize = 0x03;
/// `FUTEX`.
pub const FUTEX: usize = 0x22;
/// `MAP`.
pub const MAP: usize = 0x40;
/// `UNMAP`.
pub const UNMAP: usize = 0x41;
/// `PROTECT`.
pub const PROTECT: usize = 0x42;
/// `OPEN`.
pub const OPEN: usize = 0x61;
/// `READ`.
pub const READ: usize = 0x62;
/// `CLOSE`.
pub const CLOSE: usize = 0x64;
/// `FSTAT`.
pub const FSTAT: usize = 0x66;

/// Raw syscall with no arguments.
///
/// # Safety
/// The caller passes a valid syscall number and satisfies its arguments.
pub unsafe fn syscall0(nr: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `rax`, result in `rax`.
        unsafe {
            asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `x8`, result in `x0`.
        unsafe {
            asm!(
                "svc 0",
                in("x8") nr,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `a7`, result in `a0`.
        unsafe {
            asm!(
                "ecall",
                in("a7") nr,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret
    }
}

/// Raw syscall with one argument.
///
/// # Safety
/// See [`syscall0`].
pub unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `rax`, arguments in `rdi..`.
        unsafe {
            asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `x8`, arguments in `x0..`.
        unsafe {
            asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `a7`, arguments in `a0..`.
        unsafe {
            asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret
    }
}

/// Raw syscall with two arguments.
///
/// # Safety
/// See [`syscall0`].
pub unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `rax`, arguments in `rdi..`.
        unsafe {
            asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `x8`, arguments in `x0..`.
        unsafe {
            asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `a7`, arguments in `a0..`.
        unsafe {
            asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret
    }
}

/// Raw syscall with three arguments.
///
/// # Safety
/// See [`syscall0`].
pub unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `rax`, arguments in `rdi..`.
        unsafe {
            asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `x8`, arguments in `x0..`.
        unsafe {
            asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `a7`, arguments in `a0..`.
        unsafe {
            asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret
    }
}

/// Raw syscall with six arguments.
///
/// # Safety
/// See [`syscall0`].
pub unsafe fn syscall6(
    nr: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `rax`, arguments in `rdi..r9`.
        unsafe {
            asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                in("r10") a3,
                in("r8") a4,
                in("r9") a5,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `x8`, arguments in `x0..x5`.
        unsafe {
            asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                in("x4") a4,
                in("x5") a5,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: isize;
        // SAFETY: the syscall convention is number in `a7`, arguments in `a0..a5`.
        unsafe {
            asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                in("a5") a5,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret
    }
}

/// Exit the process.
pub fn exit(code: i32) -> ! {
    // SAFETY: EXIT never returns.
    unsafe {
        let _ = syscall1(EXIT, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Write bytes to the debug console.
pub fn debug_write(bytes: &[u8]) {
    // SAFETY: the slice is valid for the syscall duration.
    unsafe {
        let _ = syscall2(DEBUG_WRITE, bytes.as_ptr() as usize, bytes.len());
    }
}
