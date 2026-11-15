// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2a process-identity fixture.
//!
//! `/init` creates a shared control page, forks a child and starts its thread. The child reads its
//! inherited identity, exercises the bounded `setpgid`/`setsid`/`setcred` rules, writes the result
//! words to the shared page and exits. The parent reaps it and prints one deterministic line.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const CONTROL_ADDR: usize = 0x4000_0000;
const PROC_FORK: usize = 0x39;
const PROC_SELF_INFO: usize = 0x35;
const PROC_SETPGID: usize = 0x36;
const PROC_SETSID: usize = 0x37;
const PROC_SETCRED: usize = 0x38;

const SLOT_PID: usize = 0;
const SLOT_PPID: usize = 1;
const SLOT_PGID: usize = 2;
const SLOT_SID: usize = 3;
const SLOT_SETPGID: usize = 8;
const SLOT_SETSID: usize = 9;
const SLOT_SETUID0: usize = 10;
const SLOT_SETUID1: usize = 11;
const SLOT_AFTER_PGID: usize = 12;
const SLOT_MODE: usize = 13;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_identity_child_entry
    .type proc_identity_child_entry, @function
proc_identity_child_entry:
    call proc_identity_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_identity_child_entry, . - proc_identity_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_identity_child_entry
    .type proc_identity_child_entry, %function
proc_identity_child_entry:
    bl proc_identity_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_identity_child_entry, . - proc_identity_child_entry
"#
);

unsafe extern "C" {
    fn proc_identity_child_entry();
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "aarch64")]
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

fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn read64(address: usize) -> u64 {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write64(address: usize, value: u64) {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn status(raw: isize) -> u64 {
    if raw < 0 {
        raw.unsigned_abs() as u64
    } else {
        0
    }
}

/// Child thread body: identity checks, bounded setpgid/setsid/setcred, shared result words.
#[unsafe(no_mangle)]
pub extern "C" fn proc_identity_child_body() -> i32 {
    let mut values = [0u64; 8];
    let raw = unsafe {
        syscall6(
            PROC_SELF_INFO,
            values.as_mut_ptr() as usize,
            values.len(),
            0,
            0,
            0,
            0,
        )
    };
    if raw < 0 {
        return 1;
    }
    for (index, value) in values.iter().enumerate() {
        write64(CONTROL_ADDR + index * 8, *value);
    }
    let pid = values[SLOT_PID];
    let ppid = values[SLOT_PPID];
    let pgid = values[SLOT_PGID];
    let sid = values[SLOT_SID];
    if pid <= 1 || ppid == 0 || pgid != sid {
        return 2;
    }

    let mode = read64(CONTROL_ADDR + SLOT_MODE * 8);
    if mode == 0 {
        let setpgid = unsafe { syscall6(PROC_SETPGID, 0, 0, 0, 0, 0, 0) };
        write64(CONTROL_ADDR + SLOT_SETPGID * 8, status(setpgid));
        let setsid = unsafe { syscall6(PROC_SETSID, 0, 0, 0, 0, 0, 0) };
        write64(CONTROL_ADDR + SLOT_SETSID * 8, status(setsid));
    } else if mode == 1 {
        let setsid = unsafe { syscall6(PROC_SETSID, 0, 0, 0, 0, 0, 0) };
        write64(CONTROL_ADDR + SLOT_SETSID * 8, status(setsid));
        let setpgid = unsafe { syscall6(PROC_SETPGID, 0, 0, 0, 0, 0, 0) };
        write64(CONTROL_ADDR + SLOT_SETPGID * 8, status(setpgid));
    } else {
        return 4;
    }
    let setuid0 = unsafe { syscall6(PROC_SETCRED, 0, 0, 0, 0, 0, 0) };
    write64(CONTROL_ADDR + SLOT_SETUID0 * 8, status(setuid0));
    let setuid1 = unsafe { syscall6(PROC_SETCRED, 0, 1, 0, 0, 0, 0) };
    write64(CONTROL_ADDR + SLOT_SETUID1 * 8, status(setuid1));

    let mut after = [0u64; 8];
    let raw = unsafe {
        syscall6(
            PROC_SELF_INFO,
            after.as_mut_ptr() as usize,
            after.len(),
            0,
            0,
            0,
            0,
        )
    };
    if raw < 0 {
        return 3;
    }
    write64(CONTROL_ADDR + SLOT_AFTER_PGID * 8, after[SLOT_PGID]);
    0
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn map_raw(
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

fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn thread_create(
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

fn proc_wait(process: Handle, code: &mut i32) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

fn main(_args: raven_rt::Args) -> i32 {
    let control = match memobj_create(PAGE) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if map_raw(
        Handle::SELF_PROC,
        control,
        0,
        CONTROL_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )
    .is_err()
    {
        println!("proc-identity: fail control map");
        return 1;
    }
    let expected_eperm = u64::from(Errno::EPERM.0);
    let mut first = None;
    for mode in 0u64..2 {
        write64(CONTROL_ADDR + SLOT_MODE * 8, mode);
        let child = match proc_fork(Handle::SELF_PROC) {
            Ok(handle) => handle,
            Err(_) => {
                println!("proc-identity: fail fork");
                return 1;
            }
        };
        let entry = proc_identity_child_entry as *const () as usize;
        let stack = stack_top();
        if thread_create(child, entry, stack, 0).is_err() {
            println!("proc-identity: fail thread");
            return 1;
        }
        let mut code = 0i32;
        if proc_wait(child, &mut code).is_err() || code != 0 {
            println!("proc-identity: fail child code={code}");
            return 1;
        }

        let pid = read64(CONTROL_ADDR + SLOT_PID * 8);
        let ppid = read64(CONTROL_ADDR + SLOT_PPID * 8);
        let pgid = read64(CONTROL_ADDR + SLOT_PGID * 8);
        let sid = read64(CONTROL_ADDR + SLOT_SID * 8);
        let setpgid = read64(CONTROL_ADDR + SLOT_SETPGID * 8);
        let setsid = read64(CONTROL_ADDR + SLOT_SETSID * 8);
        let setuid0 = read64(CONTROL_ADDR + SLOT_SETUID0 * 8);
        let setuid1 = read64(CONTROL_ADDR + SLOT_SETUID1 * 8);
        let after_pgid = read64(CONTROL_ADDR + SLOT_AFTER_PGID * 8);
        let expected_setpgid = if mode == 0 { 0 } else { expected_eperm };
        let expected_setsid = if mode == 0 { expected_eperm } else { 0 };
        if pid <= 1
            || ppid != 1
            || pgid != 1
            || sid != 1
            || setpgid != expected_setpgid
            || setsid != expected_setsid
            || setuid0 != 0
            || setuid1 != expected_eperm
            || after_pgid != pid
        {
            println!("proc-identity: fail checks mode={mode}");
            return 1;
        }
        if mode == 0 {
            first = Some((pid, ppid, pgid, sid));
        }
    }
    let Some((pid, ppid, pgid, sid)) = first else {
        println!("proc-identity: fail first child");
        return 1;
    };
    println!("proc-identity: ok pid={pid} ppid={ppid} pgid={pgid} sid={sid}");
    0
}
