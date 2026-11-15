// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S51 PG-13 fixture.
//!
//! Proves the kernel side of the MAP/UNMAP/PROTECT contract the libc gate needs:
//! non-fixed and fixed `PROT_NONE` reservations, `PROTECT` splitting an anonymous and a
//! private memory-object mapping, partial `UNMAP` preserving the other pages, atomic
//! `FIXED | REPLACE` over a `PROT_NONE` reservation (whole range and sub-range), the
//! refusal to replace real mappings, and a mapping surviving its memory-object handle.

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
const BASE: usize = 0x6000_0000;

const MARK: u64 = 0x51ed_0000_0000_0001;

/// Raw `proc_fork(src, flags) -> Handle` number (the ABI crate is reviewer-owned).
const PROC_FORK: usize = 0x39;

/// A `PROT_NONE` page a forked child touches; the child must die with `-11`.
const FAULT_ADDR: usize = BASE + 0x50000;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global libc_map_probe_entry
    .type libc_map_probe_entry, @function
libc_map_probe_entry:
    mov rax, qword ptr [0x60050000]
    mov edi, 7
    mov eax, 0x21
    syscall
    ud2
    .size libc_map_probe_entry, . - libc_map_probe_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global libc_map_probe_entry
    .type libc_map_probe_entry, %function
libc_map_probe_entry:
    mov x9, #0x60050000
    ldr x10, [x9]
    mov x0, #7
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size libc_map_probe_entry, . - libc_map_probe_entry
"#
);

unsafe extern "C" {
    fn libc_map_probe_entry();
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

fn map_raw(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the handles and user range.
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

fn unmap_raw(address: usize, len: usize) -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user range.
    result(unsafe {
        syscall6(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            len,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn protect_raw(address: usize, len: usize, protection: usize) -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user range and protection.
    result(unsafe {
        syscall6(
            Syscall::PROTECT as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            len,
            protection,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: the length is a scalar and the kernel validates it.
    result(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn handle_close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: the handle value is copied from a live handle; no pointer is dereferenced.
    result(unsafe {
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

fn map_anon(address: usize, len: usize, protection: usize, flags: usize) -> Result<usize, Errno> {
    map_raw(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        address,
        len,
        map::pack(protection, flags),
    )
}

fn map_object(
    object: Handle,
    address: usize,
    len: usize,
    protection: usize,
    flags: usize,
) -> Result<usize, Errno> {
    map_raw(
        Handle::SELF_PROC,
        object,
        0,
        address,
        len,
        map::pack(protection, flags),
    )
}

fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    // SAFETY: scalar syscall arguments; the kernel clones a process the caller owns.
    result(unsafe { syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    // SAFETY: the kernel validates the process handle, the entry point and the stack top.
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

fn proc_wait(process: Handle, code: &mut i32) -> Result<(), Errno> {
    // SAFETY: `code` is a live writable i32 for the duration of the call.
    result(unsafe {
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

fn read64(address: usize) -> u64 {
    // SAFETY: the caller only reads a page the test currently maps readable.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write64(address: usize, value: u64) {
    // SAFETY: the caller only writes a page the test currently maps writable.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn check(condition: bool, message: &'static str) -> Result<(), &'static str> {
    if condition { Ok(()) } else { Err(message) }
}

/// `MAP` with `prot == 0`, `PROTECT` split, partial `UNMAP`, whole-range `REPLACE`.
fn reservation(base: usize) -> Result<(), &'static str> {
    // A non-fixed PROT_NONE reservation can be promoted to READ without moving.
    let hint = map_anon(0, PAGE, 0, 0).map_err(|_| "prot-none hint map")?;
    check(hint.is_multiple_of(PAGE), "prot-none hint aligned")?;
    protect_raw(hint, PAGE, prot::READ).map_err(|_| "prot-none hint protect")?;
    check(read64(hint) == 0, "prot-none hint zero")?;
    unmap_raw(hint, PAGE).map_err(|_| "prot-none hint unmap")?;

    // Fixed three-page reservation.
    check(
        map_anon(base, 3 * PAGE, 0, map::FIXED) == Ok(base),
        "prot-none fixed map",
    )?;

    // PROTECT of the middle page splits the VMA: only it becomes readable.
    protect_raw(base + PAGE, PAGE, prot::READ).map_err(|_| "protect middle read")?;
    check(read64(base + PAGE) == 0, "protect middle zero")?;
    protect_raw(base + PAGE, PAGE, prot::READ | prot::WRITE)
        .map_err(|_| "protect middle read-write")?;
    write64(base + PAGE, MARK);
    check(read64(base + PAGE) == MARK, "protect middle marker")?;

    // Partial UNMAP removes only the middle page; the two reservations stay mapped.
    unmap_raw(base + PAGE, PAGE).map_err(|_| "partial unmap")?;
    check(
        map_anon(base + PAGE, PAGE, prot::READ | prot::WRITE, map::FIXED) == Ok(base + PAGE),
        "partial unmap hole remap",
    )?;
    write64(base + PAGE, MARK + 1);
    check(
        map_anon(base, PAGE, prot::READ | prot::WRITE, map::FIXED) == Err(Errno::EEXIST),
        "left page still mapped",
    )?;
    check(
        map_anon(base + 2 * PAGE, PAGE, prot::READ | prot::WRITE, map::FIXED) == Err(Errno::EEXIST),
        "right page still mapped",
    )?;

    // REPLACE over a range containing a real mapping is refused, atomically.
    check(
        map_anon(
            base,
            3 * PAGE,
            prot::READ | prot::WRITE,
            map::FIXED | map::REPLACE,
        ) == Err(Errno::EEXIST),
        "replace refuses real mapping",
    )?;

    // Restore the middle reservation and replace the whole reservation in one call.
    unmap_raw(base + PAGE, PAGE).map_err(|_| "restore middle unmap")?;
    check(
        map_anon(base + PAGE, PAGE, 0, map::FIXED) == Ok(base + PAGE),
        "restore middle reservation",
    )?;
    check(
        map_anon(
            base,
            3 * PAGE,
            prot::READ | prot::WRITE,
            map::FIXED | map::REPLACE,
        ) == Ok(base),
        "replace whole reservation",
    )?;
    write64(base, MARK);
    write64(base + PAGE, MARK + 1);
    write64(base + 2 * PAGE, MARK + 2);
    check(read64(base) == MARK, "replace page 0")?;
    check(read64(base + PAGE) == MARK + 1, "replace page 1")?;
    check(read64(base + 2 * PAGE) == MARK + 2, "replace page 2")?;

    // Once replaced it is a normal mapping and cannot be replaced again.
    match map_anon(base, PAGE, prot::READ, map::FIXED | map::REPLACE) {
        Err(error) if error == Errno::EEXIST => {}
        Ok(_) => return Err("replace after replace succeeded"),
        Err(_) => return Err("replace after replace wrong error"),
    }
    // REPLACE without FIXED is invalid.
    check(
        map_anon(0, PAGE, prot::READ, map::REPLACE) == Err(Errno::EINVAL),
        "replace requires fixed",
    )?;
    unmap_raw(base, 3 * PAGE).map_err(|_| "reservation cleanup")?;

    // Sub-range REPLACE leaves the rest of the reservation intact.
    let sub = base + 0x40000;
    check(
        map_anon(sub, 3 * PAGE, 0, map::FIXED) == Ok(sub),
        "sub reservation map",
    )?;
    check(
        map_anon(
            sub + PAGE,
            PAGE,
            prot::READ | prot::WRITE,
            map::FIXED | map::REPLACE,
        ) == Ok(sub + PAGE),
        "sub replace middle",
    )?;
    write64(sub + PAGE, MARK);
    check(
        map_anon(
            sub,
            PAGE,
            prot::READ | prot::WRITE,
            map::FIXED | map::REPLACE,
        ) == Ok(sub),
        "sub replace left",
    )?;
    check(
        map_anon(
            sub + 2 * PAGE,
            PAGE,
            prot::READ | prot::WRITE,
            map::FIXED | map::REPLACE,
        ) == Ok(sub + 2 * PAGE),
        "sub replace right",
    )?;
    check(read64(sub + PAGE) == MARK, "sub replace contents")?;
    unmap_raw(sub, 3 * PAGE).map_err(|_| "sub reservation cleanup")?;
    Ok(())
}

/// A user read of a `PROT_NONE` page in a forked child must be a fatal fault.
fn fault_kill() -> Result<(), &'static str> {
    const _: () = assert!(FAULT_ADDR == 0x6005_0000);
    check(
        map_anon(FAULT_ADDR, PAGE, 0, map::FIXED) == Ok(FAULT_ADDR),
        "fault reservation map",
    )?;
    let child = proc_fork(Handle::SELF_PROC).map_err(|_| "fault fork")?;
    thread_create(
        child,
        libc_map_probe_entry as *const () as usize,
        stack_top(),
        0,
    )
    .map_err(|_| "fault child thread")?;
    let mut code = 0i32;
    proc_wait(child, &mut code).map_err(|_| "fault child wait")?;
    // If the supervisor leaf leaked access, the child would exit 7 instead.
    check(code == -11, "prot-none child survived the read")?;
    unmap_raw(FAULT_ADDR, PAGE).map_err(|_| "fault cleanup")?;
    Ok(())
}

/// Partial `UNMAP` keeps the surviving pages and their contents.
fn partial_unmap(base: usize) -> Result<(), &'static str> {
    check(
        map_anon(base, 4 * PAGE, prot::READ | prot::WRITE, map::FIXED) == Ok(base),
        "partial unmap map",
    )?;
    for index in 0..4 {
        write64(base + index * PAGE, MARK + index as u64);
    }
    unmap_raw(base + PAGE, 2 * PAGE).map_err(|_| "partial unmap middle two")?;
    check(read64(base) == MARK, "partial unmap left content")?;
    check(
        read64(base + 3 * PAGE) == MARK + 3,
        "partial unmap right content",
    )?;
    check(
        map_anon(base + PAGE, 2 * PAGE, prot::READ | prot::WRITE, map::FIXED) == Ok(base + PAGE),
        "partial unmap hole map",
    )?;
    check(read64(base + PAGE) == 0, "partial unmap hole zero")?;
    check(read64(base + 2 * PAGE) == 0, "partial unmap hole zero 2")?;
    check(read64(base) == MARK, "partial unmap left survives")?;
    check(
        read64(base + 3 * PAGE) == MARK + 3,
        "partial unmap right survives",
    )?;
    unmap_raw(base, 4 * PAGE).map_err(|_| "partial unmap cleanup")?;
    Ok(())
}

/// `PROTECT` of one page of a private memory-object mapping, then handle lifetime.
fn private_object(base: usize) -> Result<(), &'static str> {
    let object = memobj_create(3 * PAGE).map_err(|_| "object create")?;
    check(
        map_object(object, base, 3 * PAGE, prot::READ | prot::WRITE, map::FIXED) == Ok(base),
        "object map",
    )?;
    for index in 0..3 {
        write64(base + index * PAGE, MARK + index as u64);
    }

    // The middle page becomes read-only; the private COW contents survive the split.
    protect_raw(base + PAGE, PAGE, prot::READ).map_err(|_| "object protect read")?;
    check(read64(base + PAGE) == MARK + 1, "object middle content")?;
    write64(base, MARK + 10);
    write64(base + 2 * PAGE, MARK + 12);
    check(read64(base + PAGE) == MARK + 1, "object middle unchanged")?;

    // Promote it again and write; the two other pages keep their values.
    protect_raw(base + PAGE, PAGE, prot::READ | prot::WRITE)
        .map_err(|_| "object protect read-write")?;
    write64(base + PAGE, MARK + 21);
    check(read64(base) == MARK + 10, "object left after promote")?;
    check(
        read64(base + PAGE) == MARK + 21,
        "object middle after promote",
    )?;
    check(
        read64(base + 2 * PAGE) == MARK + 12,
        "object right after promote",
    )?;

    // Closing the handle must not invalidate the mapping (the VMA holds the object).
    handle_close(object).map_err(|_| "object close")?;
    check(read64(base) == MARK + 10, "object left after close")?;
    check(
        read64(base + PAGE) == MARK + 21,
        "object middle after close",
    )?;
    check(
        read64(base + 2 * PAGE) == MARK + 12,
        "object right after close",
    )?;
    write64(base + 2 * PAGE, MARK + 32);
    check(
        read64(base + 2 * PAGE) == MARK + 32,
        "object write after close",
    )?;
    unmap_raw(base, 3 * PAGE).map_err(|_| "object unmap")?;
    Ok(())
}

/// Holes are `EINVAL` for `UNMAP` and `ENOMEM` for `PROTECT`.
fn holes(base: usize) -> Result<(), &'static str> {
    check(
        map_anon(base, 3 * PAGE, prot::READ | prot::WRITE, map::FIXED) == Ok(base),
        "hole map",
    )?;
    unmap_raw(base + PAGE, PAGE).map_err(|_| "hole unmap")?;
    check(
        protect_raw(base, 3 * PAGE, prot::READ) == Err(Errno::ENOMEM),
        "protect hole",
    )?;
    check(
        unmap_raw(base, 3 * PAGE) == Err(Errno::EINVAL),
        "unmap hole",
    )?;
    unmap_raw(base, PAGE).map_err(|_| "hole left unmap")?;
    unmap_raw(base + 2 * PAGE, PAGE).map_err(|_| "hole right unmap")?;
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(message) = reservation(BASE) {
        println!("libc-map: fail reservation {}", message);
        return 1;
    }
    if let Err(message) = fault_kill() {
        println!("libc-map: fail fault-kill {}", message);
        return 1;
    }
    if let Err(message) = partial_unmap(BASE + 0x10000) {
        println!("libc-map: fail partial-unmap {}", message);
        return 1;
    }
    if let Err(message) = private_object(BASE + 0x20000) {
        println!("libc-map: fail private-object {}", message);
        return 1;
    }
    if let Err(message) = holes(BASE + 0x30000) {
        println!("libc-map: fail holes {}", message);
        return 1;
    }
    println!("libc-map: ok");
    0
}
