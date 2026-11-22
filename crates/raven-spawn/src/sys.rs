// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The small raw-syscall surface the loader needs.
//!
//! The stubs are the same raven-abi convention as `userland/rt/src/sys.rs`; keeping them here
//! lets the shared loader stay independent of the libc syscall crate (whose host-fake feature
//! links `std` into the runtime graph).

#![allow(clippy::missing_safety_doc)] // every raw wrapper shares syscall0's safety contract.

#[cfg(target_os = "none")]
use raven_abi::errno::Errno;
#[cfg(target_os = "none")]
use raven_abi::syscall::Syscall;
#[cfg(target_os = "none")]
use raven_abi::types::Handle;
#[cfg(target_os = "none")]
use raven_abi::types::Timespec;

/// Raw syscall with one argument.
///
/// # Safety
///
/// Uses the same register convention as the other raw wrappers.
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
/// Uses the same register convention as the other raw wrappers.
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
/// Uses the same register convention as the other raw wrappers.
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
/// Uses the same register convention as the other raw wrappers.
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
/// Uses the same register convention as the other raw wrappers.
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
/// Uses the same register convention as the other raw wrappers.
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

/// Raw `GETRANDOM` (the kernel's boot-seeded pool).
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
const GETRANDOM: usize = 0x08;

/// Fill `buffer` from the kernel entropy pool.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub fn getrandom(buffer: &mut [u8]) -> Result<(), Errno> {
    // SAFETY: GETRANDOM writes at most `buffer.len()` bytes to the caller's live slice.
    let raw = unsafe { syscall3(GETRANDOM, buffer.as_mut_ptr() as usize, buffer.len(), 0) };
    Errno::from_ret(raw)?;
    Ok(())
}

/// Read a clock (`clock::REALTIME` or `clock::MONOTONIC`).
#[cfg(target_os = "none")]
pub fn clock_get(clock_id: u32) -> Result<Timespec, Errno> {
    let mut timespec = Timespec::default();
    // SAFETY: CLOCK_GET writes one `Timespec` into the valid `&mut timespec` pointer.
    let raw = unsafe {
        syscall2(
            Syscall::CLOCK_GET as usize,
            clock_id as usize,
            &mut timespec as *mut Timespec as usize,
        )
    };
    Errno::from_ret(raw)?;
    Ok(timespec)
}

/// Raw `PROC_HANDLE_INFO` (P9b): map a process handle to its kernel pid.
#[cfg(target_os = "none")]
pub const PROC_HANDLE_INFO: usize = 0x3E;

/// Raw `PROC_PID_INFO` (P2a): a process's identity words (`pid == 0` means the caller).
#[cfg(target_os = "none")]
pub const PROC_PID_INFO: usize = 0x3A;

/// Map a process handle owned by the caller to its pid.
#[cfg(target_os = "none")]
pub fn pid_for_handle(handle: u32) -> Result<i32, Errno> {
    // SAFETY: PROC_HANDLE_INFO takes one scalar handle and returns the pid, or a negated errno.
    let raw = unsafe { syscall1(PROC_HANDLE_INFO, handle as usize) };
    let value = Errno::from_ret(raw)?;
    i32::try_from(value).map_err(|_| Errno::EINVAL)
}

#[cfg(target_os = "none")]
fn read_ids(pid: i32) -> Result<crate::ProcessIdentity, Errno> {
    let mut words = [0u64; 8];
    // SAFETY: PROC_PID_INFO writes at most eight u64 identity words into the live array.
    let raw = unsafe {
        syscall3(
            PROC_PID_INFO,
            pid as u32 as usize,
            words.as_mut_ptr() as usize,
            words.len(),
        )
    };
    Errno::from_ret(raw)?;
    Ok(crate::ProcessIdentity {
        uid: u32::try_from(words[4]).map_err(|_| Errno::EINVAL)?,
        euid: u32::try_from(words[5]).map_err(|_| Errno::EINVAL)?,
        gid: u32::try_from(words[6]).map_err(|_| Errno::EINVAL)?,
        egid: u32::try_from(words[7]).map_err(|_| Errno::EINVAL)?,
    })
}

/// Read the caller's final identity.
#[cfg(target_os = "none")]
pub fn self_ids() -> Result<crate::ProcessIdentity, Errno> {
    read_ids(0)
}

/// Read a child's final identity; the child must have no thread yet.
#[cfg(target_os = "none")]
pub fn child_ids(handle: Handle) -> Result<crate::ProcessIdentity, Errno> {
    read_ids(pid_for_handle(handle.0)?)
}

/// Wait for `handle` to exit and return the kernel's raw exit code.
#[cfg(target_os = "none")]
pub fn wait_process(handle: u32) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT takes a process handle and a writable i32 pointer owned by the caller.
    let raw = unsafe {
        syscall2(
            Syscall::PROC_WAIT as usize,
            handle as usize,
            &mut code as *mut i32 as usize,
        )
    };
    Errno::from_ret(raw)?;
    Ok(code)
}

/// Raw `PROC_WAIT_EX` (0x48): idtype/id/options select a child and return pid + status.
#[cfg(target_os = "none")]
pub const PROC_WAIT_EX: usize = 0x48;

/// Wait for a child with `idtype`/`id`/`options`; a `WNOHANG` miss returns pid 0.
#[cfg(target_os = "none")]
pub fn wait_ex(idtype: u32, id: i32, options: u32) -> Result<(i32, i32), Errno> {
    let mut status = 0i32;
    // SAFETY: PROC_WAIT_EX takes scalar selectors and a writable i32 status pointer.
    let raw = unsafe {
        syscall4(
            PROC_WAIT_EX,
            idtype as usize,
            id as isize as usize,
            options as usize,
            &mut status as *mut i32 as usize,
        )
    };
    let pid = Errno::from_ret(raw)?;
    let pid = i32::try_from(pid).map_err(|_| Errno::EINVAL)?;
    Ok((pid, status))
}

/// Close one process handle.
#[cfg(target_os = "none")]
pub fn close_handle(handle: u32) -> Result<(), Errno> {
    // SAFETY: HANDLE_CLOSE takes a scalar handle.
    let raw = unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle as usize) };
    Errno::from_ret(raw).map(|_| ())
}
