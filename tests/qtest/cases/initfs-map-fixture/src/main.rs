// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K140 initfs mapping probe.
//!
//! Maps `Handle::INITFS` through the ordinary `MAP` path, checks private COW and RX behavior,
//! exercises the rejected forms, and measures the first-touch cost over a 3 MiB range.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, clock, map, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
/// 3 MiB = 768 pages, the large-library first-touch range.
const FAULT_LEN: usize = 3 * 1024 * 1024;
const FAULT_PAGES: usize = FAULT_LEN / PAGE;
/// Raw `PROC_FORK(src, flags)` number.
const PROC_FORK: usize = 0x39;
const PARENT_VALUE: u8 = 0x50;
const CHILD_VALUE: u8 = 0x43;

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
    // SAFETY: scalar arguments; the kernel validates handles and user pointers.
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
    // SAFETY: scalar arguments; the kernel validates handles and user pointers.
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
    protection: usize,
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
            map::pack(protection, flags),
        )
    })
}

fn unmap_raw(address: usize, len: usize) -> Result<(), Errno> {
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

fn fork_raw() -> Result<Handle, Errno> {
    result(unsafe { syscall6(PROC_FORK, Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn wait_raw(child: Handle, code: &mut i32) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::PROC_WAIT as usize,
            child.0 as usize,
            code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn read8(address: usize) -> u8 {
    // SAFETY: the caller only reads a byte from a mapped, readable page.
    unsafe { core::ptr::read_volatile(address as *const u8) }
}

fn write8(address: usize, value: u8) {
    // SAFETY: the caller only writes a byte to a mapped, writable page.
    unsafe {
        core::ptr::write_volatile(address as *mut u8, value);
    }
}

fn now_ns() -> u64 {
    let time = match sys::clock_get(clock::MONOTONIC) {
        Ok(time) => time,
        Err(_) => return 0,
    };
    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
    let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
    seconds.saturating_mul(1_000_000_000).saturating_add(nanos)
}

fn failure(message: &'static str) -> i32 {
    println!("initfs-map: fail {message}");
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(_) => return failure("map_initfs"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return failure("parse initfs"),
    };
    let blob = match initfs.get("/blob") {
        Some(entry) => entry,
        None => return failure("missing /blob"),
    };
    if blob.data.len() < FAULT_LEN {
        return failure("blob shorter than 3 MiB");
    }
    let image_byte = image[0];

    let ro = match map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ as usize,
        0,
    ) {
        Ok(address) => address,
        Err(_) => return failure("read-only initfs map"),
    };
    if read8(ro) != image_byte {
        return failure("read-only byte mismatch");
    }

    let rx = match map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        (prot::READ | prot::EXEC) as usize,
        0,
    ) {
        Ok(address) => address,
        Err(_) => return failure("RX initfs map"),
    };
    if read8(rx) != image_byte {
        return failure("RX byte mismatch");
    }

    let cow = match map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        (prot::READ | prot::WRITE) as usize,
        0,
    ) {
        Ok(address) => address,
        Err(_) => return failure("COW initfs map"),
    };
    if read8(cow) != image_byte {
        return failure("COW initial byte mismatch");
    }

    let child = match fork_raw() {
        Ok(child) => child,
        Err(_) => return failure("fork"),
    };
    if child.0 == 0 {
        write8(cow, CHILD_VALUE);
        if read8(cow) != CHILD_VALUE {
            sys::exit(2);
        }
        sys::exit(0);
    }
    let mut child_code = 0i32;
    if wait_raw(child, &mut child_code).is_err() || child_code != 0 {
        return failure("child COW write");
    }
    if read8(cow) != image_byte {
        return failure("child write leaked into parent");
    }

    // A second read-only mapping is a separate view of the immutable source.
    let ro2 = match map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ as usize,
        0,
    ) {
        Ok(address) => address,
        Err(_) => return failure("second read-only map"),
    };
    write8(cow, PARENT_VALUE);
    if read8(cow) != PARENT_VALUE {
        return failure("parent COW write");
    }
    if image[0] != image_byte || read8(ro2) != image_byte {
        return failure("initfs source changed");
    }

    // Rejected forms.
    if protect_raw(ro, PAGE, (prot::READ | prot::WRITE) as usize) != Err(Errno::EACCES) {
        return failure("protect added write to initfs");
    }
    if map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        (prot::WRITE | prot::EXEC) as usize,
        0,
    ) != Err(Errno::EINVAL)
    {
        return failure("W|X initfs map accepted");
    }
    if map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ as usize,
        map::SHARED,
    ) != Err(Errno::EINVAL)
    {
        return failure("shared initfs map accepted");
    }
    if map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        1,
        0,
        PAGE,
        prot::READ as usize,
        0,
    ) != Err(Errno::EINVAL)
    {
        return failure("unaligned initfs map accepted");
    }

    let no_map = match sys::handle_transfer(
        Handle::SELF_PROC,
        Handle::INITFS,
        Rights::READ.union(Rights::EXEC),
    ) {
        Ok(handle) => handle,
        Err(_) => return failure("transfer without MAP"),
    };
    if map_raw(
        Handle::SELF_PROC,
        no_map,
        0,
        0,
        PAGE,
        prot::READ as usize,
        0,
    ) != Err(Errno::EACCES)
    {
        return failure("missing MAP right accepted");
    }
    let _ = sys::handle_close(no_map);

    let no_exec = match sys::handle_transfer(
        Handle::SELF_PROC,
        Handle::INITFS,
        Rights::READ.union(Rights::MAP),
    ) {
        Ok(handle) => handle,
        Err(_) => return failure("transfer without EXEC"),
    };
    if map_raw(
        Handle::SELF_PROC,
        no_exec,
        0,
        0,
        PAGE,
        (prot::READ | prot::EXEC) as usize,
        0,
    ) != Err(Errno::EACCES)
    {
        return failure("missing EXEC right accepted");
    }
    let _ = sys::handle_close(no_exec);

    let fault = match map_raw(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        FAULT_LEN,
        prot::READ as usize,
        0,
    ) {
        Ok(address) => address,
        Err(_) => return failure("first-touch map"),
    };
    let start = now_ns();
    let mut page = 0usize;
    while page < FAULT_PAGES {
        let _ = read8(fault.saturating_add(page.saturating_mul(PAGE)));
        page = page.saturating_add(1);
    }
    let elapsed = now_ns().saturating_sub(start);
    let per_page = if FAULT_PAGES == 0 {
        0
    } else {
        elapsed / (FAULT_PAGES as u64)
    };
    println!(
        "initfs-map: faults pages={FAULT_PAGES} total_us={} per_page_ns={}",
        elapsed / 1_000,
        per_page
    );

    let _ = unmap_raw(fault, FAULT_LEN);
    let _ = unmap_raw(ro2, PAGE);
    let _ = unmap_raw(cow, PAGE);
    let _ = unmap_raw(rx, PAGE);
    let _ = unmap_raw(ro, PAGE);
    println!("initfs-map: ok");
    0
}
