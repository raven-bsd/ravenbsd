// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S50a handle-copy fixture.
//!
//! The parent copies a shared memory handle into a bare child process; the
//! child maps the object through the copied handle and writes a marker the
//! parent observes. The parent also copies the child's process handle into
//! itself and uses it for `PROC_WAIT`, then exercises stale, revoked, missing
//! and zero rights.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::{HANDLE_COPY, Syscall};
use raven_abi::types::{Handle, Rights, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::RtSys;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const PARENT_DATA: usize = 0x4000_0000;
const PARENT_CTRL: usize = 0x4100_0000;
const PARENT_CODE: usize = 0x4200_0000;
const CHILD_CTRL: usize = 0x4400_0000;
const CHILD_CODE: usize = 0x4500_0000;
const MARKER: u32 = 0xfeed;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s50_child_stub
    .type s50_child_stub, @function
s50_child_stub:
    mov rbx, 0x44000000
1:
    mov eax, dword ptr [rbx]
    test eax, eax
    jz 1b
    mov rdi, 0
    mov esi, eax
    xor edx, edx
    mov r10, 0x43000000
    mov r8, 4096
    mov r9, 0x303
    mov eax, 0x40
    syscall
    mov dword ptr [0x43000000], 0xfeed
    mov dword ptr [rbx + 8], 1
    mov eax, 1
    xor edi, edi
    syscall
s50_child_stub_end:
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s50_child_stub
    .type s50_child_stub, %function
s50_child_stub:
    mov x9, #0x44000000
1:
    ldr w10, [x9]
    cbz w10, 1b
    mov x0, #0
    mov x1, x10
    mov x2, #0
    mov x3, #0x43000000
    mov x4, #4096
    mov x5, #0x303
    mov x8, #0x40
    svc #0
    mov w10, #0xfeed
    str w10, [x3]
    mov w10, #1
    str w10, [x9, #8]
    mov x8, #1
    mov x0, xzr
    svc #0
s50_child_stub_end:
"#
);

unsafe extern "C" {
    fn s50_child_stub();
    fn s50_child_stub_end();
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

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn handle_copy(dst: Handle, source: Handle, rights: Rights) -> Result<Handle, Errno> {
    result(unsafe {
        syscall6(
            HANDLE_COPY,
            dst.0 as usize,
            source.0 as usize,
            rights.bits() as usize,
            0,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

fn handle_revoke(handle: Handle, rights: Rights) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::HANDLE_REVOKE as usize,
            handle.0 as usize,
            rights.bits() as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    result(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
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
    result(unsafe {
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

fn protect_raw(process: Handle, address: usize, len: usize, protection: u8) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::PROTECT as usize,
            process.0 as usize,
            address,
            len,
            protection as usize,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn proc_create() -> Result<Handle, Errno> {
    result(unsafe { syscall6(Syscall::PROC_CREATE as usize, 0, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    result(unsafe {
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

fn write_u32(address: usize, value: u32) {
    // SAFETY: the fixture owns the mapping for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut u32, value);
    }
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: the fixture owns the mapping for the duration.
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn main(_args: raven_rt::Args) -> i32 {
    controller()
}

fn controller() -> i32 {
    let data = match memobj_create(PAGE) {
        Ok(object) => object,
        Err(_) => return 1,
    };
    let control = match memobj_create(PAGE) {
        Ok(object) => object,
        Err(_) => return 1,
    };
    let code = match memobj_create(PAGE) {
        Ok(object) => object,
        Err(_) => return 1,
    };
    let shared = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
    if map_raw(Handle::SELF_PROC, data, 0, PARENT_DATA, PAGE, shared).is_err()
        || map_raw(Handle::SELF_PROC, control, 0, PARENT_CTRL, PAGE, shared).is_err()
        || map_raw(Handle::SELF_PROC, code, 0, PARENT_CODE, PAGE, shared).is_err()
    {
        println!("handle-copy: parent map failed");
        return 1;
    }
    write_u32(PARENT_DATA, 0);
    write_u32(PARENT_CTRL, 0);
    write_u32(PARENT_CTRL + 8, 0);
    let stub_start = s50_child_stub as *const () as usize;
    let stub_end = s50_child_stub_end as *const () as usize;
    let stub_len = stub_end.saturating_sub(stub_start);
    // SAFETY: source is the fixture's code blob; destination is its writable shared page.
    unsafe {
        core::ptr::copy_nonoverlapping(stub_start as *const u8, PARENT_CODE as *mut u8, stub_len);
    }

    let child = match proc_create() {
        Ok(child) => child,
        Err(_) => return 1,
    };
    if map_raw(child, control, 0, CHILD_CTRL, PAGE, shared).is_err() {
        println!("handle-copy: child control map failed");
        return 1;
    }
    if map_raw(child, code, 0, CHILD_CODE, PAGE, shared).is_err() {
        println!("handle-copy: child code map failed");
        return 1;
    }
    if protect_raw(child, CHILD_CODE, PAGE, (prot::READ | prot::EXEC) as u8).is_err() {
        println!("handle-copy: child code protect failed");
        return 1;
    }
    let stack = match map_raw(
        child,
        Handle::NONE,
        0,
        0,
        2 * PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    ) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    if thread_create(child, CHILD_CODE, stack + 2 * PAGE, 0).is_err() {
        println!("handle-copy: child thread failed");
        return 1;
    }

    let child_copy = match handle_copy(
        child,
        data,
        Rights::READ
            .union(Rights::WRITE)
            .union(Rights::MAP)
            .union(Rights::DUP),
    ) {
        Ok(handle) => handle,
        Err(_) => {
            println!("handle-copy: data copy failed");
            return 1;
        }
    };
    write_u32(PARENT_CTRL, child_copy.0);
    let mut spins = 0u32;
    while (read_u32(PARENT_CTRL + 8) != 1 || read_u32(PARENT_DATA) != MARKER) && spins < 400 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    let marker = read_u32(PARENT_DATA);
    let ready = read_u32(PARENT_CTRL + 8);
    println!("handle-copy: child marker=0x{marker:x} ready={ready}");

    let process_copy =
        match handle_copy(Handle::SELF_PROC, child, Rights::READ.union(Rights::ADMIN)) {
            Ok(handle) => handle,
            Err(_) => {
                println!("handle-copy: process copy failed");
                return 1;
            }
        };
    let mut code_out = 0i32;
    let mut rt = RtSys;
    if rt.proc_wait(process_copy, &mut code_out).is_err() {
        println!("handle-copy: copied process wait failed");
        return 1;
    }
    println!("handle-copy: child status={code_out}");
    if marker != MARKER || ready != 1 || code_out != 0 {
        return 1;
    }

    let stale = match memobj_create(PAGE) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if sys::handle_close(stale).is_err() {
        return 1;
    }
    let stale_result = handle_copy(Handle::SELF_PROC, stale, Rights::READ);
    println!(
        "handle-copy: stale={}",
        if matches!(stale_result, Err(Errno::EBADF)) {
            "EBADF"
        } else {
            "unexpected"
        }
    );

    let revoked = match memobj_create(PAGE) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    if handle_revoke(revoked, Rights::WRITE).is_err() {
        return 1;
    }
    let revoked_result = handle_copy(
        Handle::SELF_PROC,
        revoked,
        Rights::READ.union(Rights::WRITE).union(Rights::MAP),
    );
    println!(
        "handle-copy: revoked={}",
        if matches!(revoked_result, Err(Errno::EPERM)) {
            "EPERM"
        } else {
            "unexpected"
        }
    );
    let missing_result = handle_copy(Handle::SELF_PROC, revoked, Rights::READ.union(Rights::EXEC));
    println!(
        "handle-copy: missing={}",
        if matches!(missing_result, Err(Errno::EACCES)) {
            "EACCES"
        } else {
            "unexpected"
        }
    );
    let zero_result = handle_copy(Handle::SELF_PROC, revoked, Rights::NONE);
    println!(
        "handle-copy: zero={}",
        if matches!(zero_result, Err(Errno::EINVAL)) {
            "EINVAL"
        } else {
            "unexpected"
        }
    );
    println!("handle-copy: done");
    0
}
