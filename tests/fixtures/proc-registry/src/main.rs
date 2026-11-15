// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2b process-registry fixture.
//!
//! `/init` forks two children. Child A is looked up and moved into its own process group by the
//! parent; child B creates a session and its group is rejected for child A. The parent also checks
//! that exited pids and unknown pids are `ESRCH`.

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
const PROC_PID_INFO: usize = 0x3A;

const SLOT_MODE: usize = 0;
const SLOT_PID: usize = 1;
const SLOT_RELEASE: usize = 2;
const SLOT_OBSERVED_PGID: usize = 3;
const SLOT_OBSERVED_B: usize = 4;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_registry_child_entry
    .type proc_registry_child_entry, @function
proc_registry_child_entry:
    call proc_registry_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_registry_child_entry, . - proc_registry_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_registry_child_entry
    .type proc_registry_child_entry, %function
proc_registry_child_entry:
    bl proc_registry_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_registry_child_entry, . - proc_registry_child_entry
"#
);

unsafe extern "C" {
    fn proc_registry_child_entry();
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

fn sleep_ms() {
    let _ = unsafe { syscall6(Syscall::NANOSLEEP as usize, 1_000_000, 0, 0, 0, 0, 0) };
}

fn self_info() -> Result<[u64; 8], Errno> {
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

fn pid_info(pid: i32) -> Result<[u64; 8], Errno> {
    let mut values = [0u64; 8];
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

fn setpgid(pid: i32, pgid: i32) -> Result<(), Errno> {
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

fn setsid() -> Result<usize, Errno> {
    checked(unsafe { syscall6(PROC_SETSID, 0, 0, 0, 0, 0, 0) })
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

fn wait_pid() -> u64 {
    for _ in 0..5000 {
        let pid = read64(CONTROL_ADDR + SLOT_PID * 8);
        if pid != 0 {
            return pid;
        }
        sleep_ms();
    }
    0
}

/// Child thread body: publish the pid, optionally create a session, then wait for the parent.
#[unsafe(no_mangle)]
pub extern "C" fn proc_registry_child_body() -> i32 {
    let mode = read64(CONTROL_ADDR + SLOT_MODE * 8);
    if mode == 1 && setsid().is_err() {
        return 1;
    }
    let info = match self_info() {
        Ok(values) => values,
        Err(_) => return 2,
    };
    write64(CONTROL_ADDR + SLOT_PID * 8, info[0]);
    while read64(CONTROL_ADDR + SLOT_RELEASE * 8) == 0 {
        sleep_ms();
    }
    let after = match self_info() {
        Ok(values) => values,
        Err(_) => return 3,
    };
    let slot = if mode == 1 {
        SLOT_OBSERVED_B
    } else {
        SLOT_OBSERVED_PGID
    };
    write64(CONTROL_ADDR + slot * 8, after[2]);
    0
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
        println!("proc-registry: fail control map");
        return 1;
    }

    // Child A: parent looks it up and moves it into its own group.
    write64(CONTROL_ADDR + SLOT_MODE * 8, 0);
    write64(CONTROL_ADDR + SLOT_RELEASE * 8, 0);
    let child_a = match proc_fork(Handle::SELF_PROC) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if thread_create(
        child_a,
        proc_registry_child_entry as *const () as usize,
        stack_top(),
        0,
    )
    .is_err()
    {
        println!("proc-registry: fail child-a thread");
        return 1;
    }
    let pid_a = wait_pid();
    let info_a = match pid_info(pid_a as i32) {
        Ok(values) => values,
        Err(_) => {
            println!("proc-registry: fail child-a lookup");
            return 1;
        }
    };
    if info_a[0] != pid_a || info_a[1] != 1 || info_a[2] != 1 || info_a[3] != 1 {
        println!("proc-registry: fail child-a identity");
        return 1;
    }
    if setpgid(pid_a as i32, pid_a as i32).is_err() {
        println!("proc-registry: fail child-a setpgid");
        return 1;
    }

    // Child B: it creates a new session; child A may not join that group.
    write64(CONTROL_ADDR + SLOT_MODE * 8, 1);
    write64(CONTROL_ADDR + SLOT_RELEASE * 8, 0);
    write64(CONTROL_ADDR + SLOT_PID * 8, 0);
    let child_b = match proc_fork(Handle::SELF_PROC) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if thread_create(
        child_b,
        proc_registry_child_entry as *const () as usize,
        stack_top(),
        0,
    )
    .is_err()
    {
        println!("proc-registry: fail child-b thread");
        return 1;
    }
    let pid_b = wait_pid();
    let info_b = match pid_info(pid_b as i32) {
        Ok(values) => values,
        Err(_) => {
            println!("proc-registry: fail child-b lookup");
            return 1;
        }
    };
    if info_b[0] != pid_b || info_b[1] != 1 || info_b[3] != pid_b {
        println!("proc-registry: fail child-b identity");
        return 1;
    }
    if !matches!(setpgid(pid_a as i32, pid_b as i32), Err(Errno::EPERM)) {
        println!("proc-registry: fail other-session group");
        return 1;
    }

    write64(CONTROL_ADDR + SLOT_RELEASE * 8, 1);
    let mut code = 0i32;
    if proc_wait(child_a, &mut code).is_err() || code != 0 {
        println!("proc-registry: fail child-a wait");
        return 1;
    }
    if proc_wait(child_b, &mut code).is_err() || code != 0 {
        println!("proc-registry: fail child-b wait");
        return 1;
    }
    if read64(CONTROL_ADDR + SLOT_OBSERVED_PGID * 8) != pid_a
        || read64(CONTROL_ADDR + SLOT_OBSERVED_B * 8) != pid_b
    {
        println!("proc-registry: fail observed groups");
        return 1;
    }
    if !matches!(pid_info(pid_a as i32), Err(Errno::ESRCH))
        || !matches!(pid_info(pid_b as i32), Err(Errno::ESRCH))
    {
        println!("proc-registry: fail exited ESRCH");
        return 1;
    }
    if !matches!(pid_info(9999), Err(Errno::ESRCH)) {
        println!("proc-registry: fail unknown pid");
        return 1;
    }

    // P2a self rules remain.
    if !matches!(setpgid(0, 0), Err(Errno::EPERM)) {
        println!("proc-registry: fail self setpgid");
        return 1;
    }
    if !matches!(setsid(), Err(Errno::EPERM)) {
        println!("proc-registry: fail self setsid");
        return 1;
    }
    println!("proc-registry: ok child-a={pid_a} child-b={pid_b}");
    0
}
