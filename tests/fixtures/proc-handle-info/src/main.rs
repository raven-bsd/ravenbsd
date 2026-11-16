// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P9b `PROC_HANDLE_INFO` and wait-status encoding fixture.
//!
//! Creates an empty child, maps its handle to a pid, terminates it with code 42, waits, checks
//! that the raw code shifts to the POSIX `42 << 8` status, keeps the pid readable after exit,
//! closes the handle and checks EBADF for a stale/non-process handle.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PROC_CREATE: usize = 0x30;
const PROC_WAIT: usize = 0x31;
const PROC_HANDLE_INFO: usize = 0x3E;

#[inline(always)]
unsafe fn syscall6(
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

fn handle(raw: isize) -> Result<Handle, Errno> {
    Errno::from_ret(raw).map(|value| Handle(value as u32))
}

fn main(_args: raven_rt::Args) -> i32 {
    let child = match handle(unsafe { syscall6(PROC_CREATE, 0, 0, 0, 0, 0, 0) }) {
        Ok(handle) => handle,
        Err(_) => return fail("create"),
    };
    let pid = match Errno::from_ret(unsafe {
        syscall6(PROC_HANDLE_INFO, child.0 as usize, 0, 0, 0, 0, 0)
    }) {
        Ok(pid) => pid,
        Err(_) => return fail("info"),
    };
    if pid <= 1 {
        return fail("pid");
    }
    println!("proc-handle-info: pid={pid} info=ok");

    if Errno::from_ret(unsafe {
        syscall6(
            Syscall::PROC_TERMINATE as usize,
            child.0 as usize,
            42,
            0,
            0,
            0,
            0,
        )
    })
    .is_err()
    {
        return fail("terminate");
    }
    let mut code = 0i32;
    if Errno::from_ret(unsafe {
        syscall6(
            PROC_WAIT,
            child.0 as usize,
            &mut code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    })
    .is_err()
    {
        return fail("wait");
    }
    let status = code << 8;
    if code != 42 || status != 10_752 {
        return fail("status");
    }
    println!("proc-handle-info: wait=42 status=10752");

    match Errno::from_ret(unsafe { syscall6(PROC_HANDLE_INFO, child.0 as usize, 0, 0, 0, 0, 0) }) {
        Ok(after) if after == pid => {}
        _ => return fail("exited info"),
    }
    println!("proc-handle-info: exited info=ok");

    if Errno::from_ret(unsafe {
        syscall6(
            Syscall::HANDLE_CLOSE as usize,
            child.0 as usize,
            0,
            0,
            0,
            0,
            0,
        )
    })
    .is_err()
    {
        return fail("close");
    }
    if Errno::from_ret(unsafe { syscall6(PROC_HANDLE_INFO, child.0 as usize, 0, 0, 0, 0, 0) })
        != Err(Errno::EBADF)
    {
        return fail("stale");
    }
    println!("proc-handle-info: stale=badf");

    let memobj =
        match handle(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, 4096, 0, 0, 0, 0, 0) }) {
            Ok(handle) => handle,
            Err(_) => return fail("memobj"),
        };
    if Errno::from_ret(unsafe { syscall6(PROC_HANDLE_INFO, memobj.0 as usize, 0, 0, 0, 0, 0) })
        != Err(Errno::EBADF)
    {
        return fail("non-process");
    }
    let _ = unsafe {
        syscall6(
            Syscall::HANDLE_CLOSE as usize,
            memobj.0 as usize,
            0,
            0,
            0,
            0,
            0,
        )
    };
    println!("proc-handle-info: nonprocess=badf");
    println!("proc-handle-info: ok");
    0
}

fn fail(step: &str) -> i32 {
    println!("proc-handle-info: fail {step}");
    1
}
