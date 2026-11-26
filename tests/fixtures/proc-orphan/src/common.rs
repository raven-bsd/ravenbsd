// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared raw-syscall helpers for the P2f fixtures.

#![allow(dead_code)]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};

pub const PAGE: usize = 4096;
pub const PROC_FORK: usize = 0x39;
pub const PROC_SELF_INFO: usize = 0x35;
pub const PROC_SETPGID: usize = 0x36;
pub const PROC_SETSID: usize = 0x37;
pub const PROC_HANDLE_INFO: usize = 0x3E;
pub const PROC_PID_INFO: usize = 0x3A;
pub const PROC_WAIT_EX: usize = 0x48;
pub const PROC_GETPGID: usize = 0x49;
pub const PROC_SIGNAL: usize = 0x4A;
pub const PROC_SIGRECV: usize = 0x4B;
pub const PROC_TIMES: usize = 0x4C;
pub const PROC_GETRUSAGE: usize = 0x4D;
pub const PROC_GETRLIMIT: usize = 0x4E;
pub const PROC_SETRLIMIT: usize = 0x4F;
pub const P_PID: usize = 1;
pub const P_PGID: usize = 2;
pub const P_ALL: usize = 3;
pub const WNOHANG: usize = 1;
pub const WUNTRACED: usize = 2;
pub const WCONTINUED: usize = 4;
pub const SIGUSR1: usize = 10;
pub const SIGUSR2: usize = 12;
pub const SIGKILL: usize = 9;
pub const SIGSTOP: usize = 19;
pub const SIGCONT: usize = 18;
pub const RLIMIT_NPROC: usize = 6;

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "aarch64")]
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
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack),
        );
    }
    result
}

pub fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

pub fn read64(address: usize) -> u64 {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

pub fn write64(address: usize, value: u64) {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

pub fn sleep_ms() {
    let _ = unsafe { syscall6(Syscall::NANOSLEEP as usize, 1_000_000, 0, 0, 0, 0, 0) };
}

pub fn memobj_create(len: usize) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

pub fn map_raw(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    checked(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            offset,
            address,
            len,
            flags,
        )
    })
}

pub fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

pub fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    checked(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack_top,
            arg,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

pub fn close(handle: Handle) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            Syscall::HANDLE_CLOSE as usize,
            handle.0 as usize,
            0,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

pub fn handle_info(handle: Handle) -> Result<usize, Errno> {
    checked(unsafe { syscall6(PROC_HANDLE_INFO, handle.0 as usize, 0, 0, 0, 0, 0) })
}

pub fn wait_ex(idtype: usize, id: i32, options: usize, status: &mut i32) -> Result<usize, Errno> {
    checked(unsafe {
        syscall6(
            PROC_WAIT_EX,
            idtype,
            id as isize as usize,
            options,
            status as *mut i32 as usize,
            0,
            0,
        )
    })
}

pub fn signal(target: i32, signo: usize) -> Result<(), Errno> {
    checked(unsafe { syscall6(PROC_SIGNAL, target as isize as usize, signo, 0, 0, 0, 0) })
        .map(|_| ())
}

pub fn sigrecv(info: &mut [u64; 3], flags: usize) -> Result<usize, Errno> {
    checked(unsafe { syscall6(PROC_SIGRECV, info.as_mut_ptr() as usize, flags, 0, 0, 0, 0) })
}

pub fn getpgid(pid: i32) -> Result<usize, Errno> {
    checked(unsafe { syscall6(PROC_GETPGID, pid as isize as usize, 0, 0, 0, 0, 0) })
}

pub fn setpgid(pid: i32, pgid: i32) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            PROC_SETPGID,
            pid as isize as usize,
            pgid as isize as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

pub fn setsid() -> Result<usize, Errno> {
    checked(unsafe { syscall6(PROC_SETSID, 0, 0, 0, 0, 0, 0) })
}

pub fn self_info() -> Result<[u64; 8], Errno> {
    let mut values = [0u64; 8];
    checked(unsafe {
        syscall6(
            PROC_SELF_INFO,
            values.as_mut_ptr() as usize,
            values.len(),
            0,
            0,
            0,
            0,
        )
    })?;
    Ok(values)
}

pub fn pid_info(pid: i32) -> Result<[u64; 8], Errno> {
    let mut values = [0u64; 8];
    // SAFETY: PROC_PID_INFO writes eight u64 identity words to the live caller-owned array.
    checked(unsafe {
        syscall6(
            PROC_PID_INFO,
            pid as isize as usize,
            values.as_mut_ptr() as usize,
            values.len(),
            0,
            0,
            0,
        )
    })?;
    Ok(values)
}

pub fn times() -> Result<[u64; 4], Errno> {
    let mut values = [0u64; 4];
    checked(unsafe {
        syscall6(
            PROC_TIMES,
            values.as_mut_ptr() as usize,
            values.len(),
            0,
            0,
            0,
            0,
        )
    })?;
    Ok(values)
}

pub fn getrusage() -> Result<[u64; 8], Errno> {
    let mut values = [0u64; 8];
    checked(unsafe {
        syscall6(
            PROC_GETRUSAGE,
            0,
            values.as_mut_ptr() as usize,
            values.len(),
            0,
            0,
            0,
        )
    })?;
    Ok(values)
}

pub fn getrlimit(resource: usize) -> Result<[u64; 2], Errno> {
    let mut values = [0u64; 2];
    checked(unsafe {
        syscall6(
            PROC_GETRLIMIT,
            resource,
            values.as_mut_ptr() as usize,
            0,
            0,
            0,
            0,
        )
    })?;
    Ok(values)
}

pub fn setrlimit(resource: usize, cur: u64, max: u64) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            PROC_SETRLIMIT,
            resource,
            cur as usize,
            max as usize,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

pub fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

pub fn control_page() -> Option<usize> {
    let control = memobj_create(PAGE).ok()?;
    map_raw(
        Handle::SELF_PROC,
        control,
        0,
        0x4000_0000,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )
    .ok()?;
    Some(0x4000_0000)
}
