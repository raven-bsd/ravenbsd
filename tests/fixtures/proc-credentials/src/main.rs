// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2c credential fixture.
//!
//! `/init` starts as root, sets and reads supplementary groups, drops uid/gid through the saved-set
//! rules, rejects privilege regain, then forks a child that observes the inherited credential set
//! through the shared control page.

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
const PROC_SETCRED_EX: usize = 0x3B;
const PROC_CRED_INFO: usize = 0x3C;
const PROC_GETGROUPS: usize = 0x3D;

const KIND_SETUID: u32 = 1;
const KIND_SETEUID: u32 = 2;
const KIND_SETREUID: u32 = 3;
const KIND_SETRESUID: u32 = 4;
const KIND_SETGID: u32 = 5;
const KIND_SETEGID: u32 = 6;
const KIND_SETREGID: u32 = 7;
const KIND_SETRESGID: u32 = 8;
const KIND_SETGROUPS: u32 = 9;

const SLOT_UID: usize = 0;
const SLOT_EUID: usize = 1;
const SLOT_SUID: usize = 2;
const SLOT_GID: usize = 3;
const SLOT_EGID: usize = 4;
const SLOT_SGID: usize = 5;
const SLOT_GROUP_COUNT: usize = 6;
const SLOT_GROUP0: usize = 7;
const SLOT_GROUP1: usize = 8;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_credentials_child_entry
    .type proc_credentials_child_entry, @function
proc_credentials_child_entry:
    call proc_credentials_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_credentials_child_entry, . - proc_credentials_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_credentials_child_entry
    .type proc_credentials_child_entry, %function
proc_credentials_child_entry:
    bl proc_credentials_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_credentials_child_entry, . - proc_credentials_child_entry
"#
);

unsafe extern "C" {
    fn proc_credentials_child_entry();
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

fn cred_info() -> Result<[u64; 6], Errno> {
    let mut values = [0u64; 6];
    checked(unsafe {
        syscall6(
            PROC_CRED_INFO,
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

fn getgroups(out: &mut [u32]) -> Result<usize, Errno> {
    checked(unsafe {
        syscall6(
            PROC_GETGROUPS,
            out.as_mut_ptr() as usize,
            out.len(),
            0,
            0,
            0,
            0,
        )
    })
}

fn setcred(kind: u32, a: i32, b: i32, c: i32, groups: &[u32]) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            PROC_SETCRED_EX,
            kind as usize,
            a as isize as usize,
            b as isize as usize,
            c as isize as usize,
            groups.as_ptr() as usize,
            groups.len(),
        )
    })
    .map(|_| ())
}

fn setgroups(groups: &[u32]) -> Result<(), Errno> {
    setcred(KIND_SETGROUPS, 0, 0, 0, groups)
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

/// Child thread body: publish the inherited credentials and group list.
#[unsafe(no_mangle)]
pub extern "C" fn proc_credentials_child_body() -> i32 {
    let creds = match cred_info() {
        Ok(values) => values,
        Err(_) => return 1,
    };
    for (slot, value) in [
        SLOT_UID, SLOT_EUID, SLOT_SUID, SLOT_GID, SLOT_EGID, SLOT_SGID,
    ]
    .into_iter()
    .zip(creds)
    {
        write64(CONTROL_ADDR + slot * 8, value);
    }
    let count = match getgroups(&mut []) {
        Ok(count) => count,
        Err(_) => return 2,
    };
    write64(CONTROL_ADDR + SLOT_GROUP_COUNT * 8, count as u64);
    if count >= 2 {
        let mut groups = [0u32; 4];
        if getgroups(&mut groups).is_err() {
            return 3;
        }
        write64(CONTROL_ADDR + SLOT_GROUP0 * 8, u64::from(groups[0]));
        write64(CONTROL_ADDR + SLOT_GROUP1 * 8, u64::from(groups[1]));
    }
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
        println!("proc-credentials: fail control map");
        return 1;
    }

    let initial = match self_info() {
        Ok(values) => values,
        Err(_) => return 1,
    };
    if initial[4] != 0 || initial[5] != 0 {
        println!("proc-credentials: fail initial");
        return 1;
    }
    if setgroups(&[11, 12]).is_err() {
        println!("proc-credentials: fail setgroups");
        return 1;
    }
    let mut groups = [0u32; 4];
    if getgroups(&mut groups) != Ok(2) || groups[0] != 11 || groups[1] != 12 {
        println!("proc-credentials: fail getgroups");
        return 1;
    }
    if getgroups(&mut [0u32; 1]) != Err(Errno::EINVAL) {
        println!("proc-credentials: fail small group buffer");
        return 1;
    }
    let oversized = [0u32; 17];
    if setgroups(&oversized) != Err(Errno::EINVAL) {
        println!("proc-credentials: fail oversized groups");
        return 1;
    }

    if setcred(KIND_SETRESGID, 1000, 1000, 1000, &[]).is_err() {
        println!("proc-credentials: fail setresgid");
        return 1;
    }
    if setcred(KIND_SETRESUID, 1000, 1000, 1000, &[]).is_err() {
        println!("proc-credentials: fail setresuid");
        return 1;
    }
    let dropped = match cred_info() {
        Ok(values) => values,
        Err(_) => return 1,
    };
    if dropped[0] != 1000
        || dropped[1] != 1000
        || dropped[2] != 1000
        || dropped[3] != 1000
        || dropped[4] != 1000
        || dropped[5] != 1000
    {
        println!("proc-credentials: fail saved ids");
        return 1;
    }

    if setcred(KIND_SETUID, 0, -1, -1, &[]) != Err(Errno::EPERM) {
        println!("proc-credentials: fail regain uid");
        return 1;
    }
    if setcred(KIND_SETEUID, 1000, -1, -1, &[]).is_err() {
        println!("proc-credentials: fail saved euid");
        return 1;
    }
    if setcred(KIND_SETRESUID, 0, 0, 0, &[]) != Err(Errno::EPERM) {
        println!("proc-credentials: fail regain resuid");
        return 1;
    }
    if setcred(KIND_SETGID, 0, -1, -1, &[]) != Err(Errno::EPERM) {
        println!("proc-credentials: fail regain gid");
        return 1;
    }
    if setcred(KIND_SETREUID, 1000, 1000, -1, &[]).is_err()
        || setcred(KIND_SETEGID, 1000, -1, -1, &[]).is_err()
        || setcred(KIND_SETREGID, 1000, 1000, -1, &[]).is_err()
    {
        println!("proc-credentials: fail saved gid");
        return 1;
    }
    if setcred(KIND_SETRESGID, 0, 0, 0, &[]) != Err(Errno::EPERM) {
        println!("proc-credentials: fail regain resgid");
        return 1;
    }
    if setgroups(&[13]) != Err(Errno::EPERM) {
        println!("proc-credentials: fail unprivileged setgroups");
        return 1;
    }
    if getgroups(&mut groups) != Ok(2) || groups[0] != 11 || groups[1] != 12 {
        println!("proc-credentials: fail retained groups");
        return 1;
    }

    // The forked child must inherit the full dropped credential set and groups.
    let child = match proc_fork(Handle::SELF_PROC) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if thread_create(
        child,
        proc_credentials_child_entry as *const () as usize,
        stack_top(),
        0,
    )
    .is_err()
    {
        println!("proc-credentials: fail child thread");
        return 1;
    }
    let mut code = 0i32;
    if proc_wait(child, &mut code).is_err() || code != 0 {
        println!("proc-credentials: fail child wait");
        return 1;
    }
    let mut ok = true;
    for slot in [
        SLOT_UID, SLOT_EUID, SLOT_SUID, SLOT_GID, SLOT_EGID, SLOT_SGID,
    ] {
        ok &= read64(CONTROL_ADDR + slot * 8) == 1000;
    }
    ok &= read64(CONTROL_ADDR + SLOT_GROUP_COUNT * 8) == 2;
    ok &= read64(CONTROL_ADDR + SLOT_GROUP0 * 8) == 11;
    ok &= read64(CONTROL_ADDR + SLOT_GROUP1 * 8) == 12;
    if !ok {
        println!("proc-credentials: fail child inheritance");
        return 1;
    }
    println!("proc-credentials: ok");
    0
}
