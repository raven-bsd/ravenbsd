// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw syscall instructions and the typed `raven-abi` wrappers.
#![allow(clippy::missing_safety_doc)] // every raw wrapper shares syscall0's safety contract.

use core::sync::atomic::AtomicU32;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, Timespec, futex};
use raven_abi::uname::Uname;

/// Raw syscall with no arguments.
///
/// # Safety
///
/// The caller must use a valid `raven-abi` syscall number and satisfy its pointer arguments.
pub unsafe fn syscall0(nr: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
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
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with one argument.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
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
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with two arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
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
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with three arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
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
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with four arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                in("r10") a3,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with five arguments.
///
/// # Safety
///
/// See [`syscall0`].
pub unsafe fn syscall5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        let ret: isize;
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") nr as isize => ret,
                in("rdi") a0,
                in("rsi") a1,
                in("rdx") a2,
                in("r10") a3,
                in("r8") a4,
                clobber_abi("C"),
                options(nostack),
            );
        }
        ret
    }
    #[cfg(target_arch = "aarch64")]
    {
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
                "svc 0",
                in("x8") nr,
                in("x0") a0,
                in("x1") a1,
                in("x2") a2,
                in("x3") a3,
                in("x4") a4,
                lateout("x0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") nr,
                in("a0") a0,
                in("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret as isize
    }
}

/// Raw syscall with six arguments.
///
/// # Safety
///
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
        // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
        // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`.
        unsafe {
            core::arch::asm!(
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
        let ret: usize;
        // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
        // in `x0..x5`, and the result in `x0`.
        unsafe {
            core::arch::asm!(
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
        ret as isize
    }
    #[cfg(target_arch = "riscv64")]
    {
        let ret: usize;
        // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
        // in `a0..a5`, and the result in `a0`.
        unsafe {
            core::arch::asm!(
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
        ret as isize
    }
}

fn checked(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

/// Terminate the calling process.
pub fn exit(code: i32) -> ! {
    // SAFETY: EXIT takes only the scalar exit code and never returns.
    unsafe {
        let _ = syscall1(Syscall::EXIT as usize, code as usize);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Write bytes to the debug console.
pub fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    // SAFETY: DEBUG_WRITE reads `bytes.len()` bytes from the slice's valid pointer.
    checked(unsafe {
        syscall2(
            Syscall::DEBUG_WRITE as usize,
            bytes.as_ptr() as usize,
            bytes.len(),
        )
    })
}

/// Read the system identification record.
pub fn uname() -> Result<Uname, Errno> {
    let mut uname = Uname::zeroed();
    // SAFETY: UNAME writes one `Uname` into the valid `&mut uname` pointer.
    checked(unsafe { syscall1(Syscall::UNAME as usize, &mut uname as *mut Uname as usize) })?;
    Ok(uname)
}

/// Read a clock (`clock::REALTIME` or `clock::MONOTONIC`).
pub fn clock_get(clock_id: u32) -> Result<Timespec, Errno> {
    let mut timespec = Timespec::default();
    // SAFETY: CLOCK_GET writes one `Timespec` into the valid `&mut timespec` pointer.
    checked(unsafe {
        syscall2(
            Syscall::CLOCK_GET as usize,
            clock_id as usize,
            &mut timespec as *mut Timespec as usize,
        )
    })?;
    Ok(timespec)
}

/// Yield the CPU to another runnable thread.
pub fn yield_now() {
    // SAFETY: YIELD takes no arguments and only yields the CPU.
    unsafe {
        let _ = syscall0(Syscall::YIELD as usize);
    }
}

/// Sleep for at least `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    // SAFETY: NANOSLEEP takes only the scalar nanosecond duration.
    checked(unsafe { syscall1(Syscall::NANOSLEEP as usize, ns as usize) })?;
    Ok(())
}

/// Wait on a futex word.
pub fn futex_wait(
    address: &AtomicU32,
    expected: u32,
    timeout_ns: Option<u64>,
) -> Result<(), Errno> {
    // SAFETY: FUTEX with WAIT reads the `AtomicU32` behind the valid `address` reference.
    checked(unsafe {
        syscall4(
            Syscall::FUTEX as usize,
            address.as_ptr() as usize,
            futex::WAIT as usize,
            expected as usize,
            timeout_ns.unwrap_or(0) as usize,
        )
    })?;
    Ok(())
}

/// Wake up to `count` futex waiters.
pub fn futex_wake(address: &AtomicU32, count: u32) -> Result<usize, Errno> {
    // SAFETY: FUTEX with WAKE reads the `AtomicU32` behind the valid `address` reference.
    checked(unsafe {
        syscall3(
            Syscall::FUTEX as usize,
            address.as_ptr() as usize,
            futex::WAKE as usize,
            count as usize,
        )
    })
}

/// Close a handle.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: HANDLE_CLOSE takes a scalar handle value.
    checked(unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) })?;
    Ok(())
}

/// Duplicate a handle with an intersected rights mask.
pub fn handle_dup(handle: Handle, rights: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_DUP takes a scalar handle and rights mask.
    let value = checked(unsafe {
        syscall2(
            Syscall::HANDLE_DUP as usize,
            handle.0 as usize,
            rights.bits() as usize,
        )
    })?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_abi::types::clock;

    #[test]
    fn clock_constants_are_the_linux_ones() {
        assert_eq!(clock::REALTIME, 0);
        assert_eq!(clock::MONOTONIC, 1);
    }

    #[test]
    fn handle_conversion_keeps_abi_bits() {
        let handle = Handle(0x1234_5678);
        assert_eq!(handle.0 as usize, 0x1234_5678);
    }
}
