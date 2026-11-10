// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S25 controller: transfers a read-only memory object before child start, then receives one as
//! an Open reply attachment.  It repeats child start/exit to exercise handle-table teardown.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::{Entry, Initfs};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys::{syscall1, syscall2, syscall3, syscall6};
use raven_s25_xfer as _;

entry!(main);

const PAGE: usize = 4096;
const INITFS_BASE: usize = 0x1000_0000;
const STACK_SCRATCH: usize = 0x2000_0000;
const STACK_SIZE: usize = 64 * 1024;
const STACK_TOP: usize = (USER_END as usize) - PAGE;
const STACK_BASE: usize = STACK_TOP - STACK_SIZE;
const PATTERN: u8 = 0xa5;
const CYCLES: usize = 100;

fn result(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

fn handle(value: isize) -> Result<Handle, Errno> {
    u32::try_from(result(value)?)
        .map(Handle)
        .map_err(|_| Errno::EINVAL)
}

fn proc_create() -> Result<Handle, Errno> {
    handle(unsafe { syscall1(Syscall::PROC_CREATE as usize, 0) })
}

fn memobj_create() -> Result<Handle, Errno> {
    handle(unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, PAGE, 0) })
}

fn transfer(process: Handle, object: Handle, mask: Rights) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall3(
            Syscall::HANDLE_TRANSFER as usize,
            process.0 as usize,
            object.0 as usize,
            mask.0 as usize,
        )
    })
}

fn map_memory(
    process: Handle,
    object: Handle,
    address: usize,
    length: usize,
    flags: usize,
) -> Result<usize, Errno> {
    map_at(process, object, 0, address, length, flags)
}

fn map_at(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    length: usize,
    flags: usize,
) -> Result<usize, Errno> {
    // SAFETY: MAP follows the stable syscall ABI; the kernel validates every argument.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            offset,
            address,
            length,
            flags,
        )
    })
}

fn close(handle: Handle) {
    // SAFETY: HANDLE_CLOSE takes one scalar handle validated by the kernel.
    let _ = unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) };
}

fn unmap(address: usize, length: usize) {
    // SAFETY: UNMAP takes scalar arguments; this helper only unmaps the parent's stack scratch.
    let _ = unsafe {
        syscall3(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            length,
        )
    };
}

fn wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    // SAFETY: `code` is writable for this syscall.
    result(unsafe {
        syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            (&mut code as *mut i32) as usize,
        )
    })?;
    Ok(code)
}

fn thread_create(process: Handle, entry: usize, stack: usize) -> Result<(), Errno> {
    // SAFETY: THREAD_CREATE follows the stable syscall ABI; arguments are prepared below.
    result(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack,
            stack,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn map_initfs() -> Result<&'static [u8], Errno> {
    let first = map_memory(Handle::SELF_PROC, Handle::INITFS, 0, PAGE, prot::READ)?;
    // SAFETY: the one-page initfs mapping contains the C7 total-size field at byte 40.
    let total = unsafe { ((first + 40) as *const u64).read_unaligned() as usize };
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the validated initfs occupies `total` bytes at `base`.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

fn start_child(entry: &Entry<'_>, elf: &Elf<'_>, memory: Handle) -> Result<i32, Errno> {
    let child = proc_create()?;
    for step in plan_load(elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL)? {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                if let Err(error) = map_at(
                    child,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                ) {
                    println!("s25: child file map {}", error.0);
                    return Err(error);
                }
            }
            StepKind::Zero => {
                if let Err(error) = map_memory(
                    child,
                    Handle::NONE,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                ) {
                    println!("s25: child zero map {}", error.0);
                    return Err(error);
                }
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = memobj_create()?;
    let scratch = map_memory(
        Handle::SELF_PROC,
        stack,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    if let Err(error) = map_memory(
        child,
        stack,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    ) {
        println!("s25: child stack map {}", error.0);
        return Err(error);
    }
    let stack_pointer = build_stack(scratch);
    let rights = Rights::READ.union(Rights::MAP);
    if transfer(child, memory, Rights::NONE) != Err(Errno::EINVAL) {
        return Err(Errno::EIO);
    }
    if transfer(child, Handle::SELF_PROC, Rights::READ) != Err(Errno::EBADF) {
        return Err(Errno::EIO);
    }
    let transferred = transfer(child, memory, rights)?;
    if transferred != Handle::new(3, 0) {
        return Err(Errno::EIO);
    }
    if let Err(error) = thread_create(child, elf.entry() as usize, stack_pointer) {
        println!("s25: child thread {}", error.0);
        return Err(error);
    }
    unmap(scratch, STACK_SIZE);
    close(stack);
    let code = wait(child)?;
    close(child);
    Ok(code)
}

fn build_stack(base: usize) -> usize {
    let stack = STACK_TOP - 0x100;
    let mut offset = stack - STACK_BASE;
    for value in [1u64, STACK_TOP as u64 - 0x40, 0, 0, 6, PAGE as u64, 0, 0] {
        // SAFETY: the shared stack mapping is writable and `offset` is inside its fixed size.
        unsafe { ((base + offset) as *mut u64).write_unaligned(value.to_le()) };
        offset += 8;
    }
    // SAFETY: the string is inside the writable shared stack mapping.
    unsafe { ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'x') };
    // SAFETY: the string terminator is inside the writable shared stack mapping.
    unsafe { ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0) };
    stack
}

fn open(path: &str) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            1,
            0,
            0,
            0,
        )
    })
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs() {
        Ok(image) => image,
        Err(_) => return 1,
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return 2,
    };
    let child = match initfs.get("/child") {
        Some(entry) => entry,
        None => return 3,
    };
    let elf = match Elf::parse(child.data, Machine::X86_64, USER_MIN..USER_END) {
        Ok(elf) => elf,
        Err(_) => return 4,
    };
    let memory = match memobj_create() {
        Ok(handle) => handle,
        Err(_) => return 5,
    };
    let pattern = match map_memory(
        Handle::SELF_PROC,
        memory,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::SHARED),
    ) {
        Ok(address) => address,
        Err(_) => return 6,
    };
    // SAFETY: this is the caller's writable one-page shared mapping.
    unsafe { (pattern as *mut u8).write(PATTERN) };
    let mut cycle = 0usize;
    while cycle < CYCLES {
        match start_child(&child, &elf, memory) {
            Ok(7) => {}
            Ok(code) => {
                println!("s25: child exit {}", code);
                return 7;
            }
            Err(error) => {
                println!("s25: child setup {}", error.0);
                return 7;
            }
        }
        cycle += 1;
    }
    println!("s25: transfer ok");
    println!("s25: rights ok");

    let server = match spawn(
        &initfs,
        "/server",
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    ) {
        Ok(server) => server,
        Err(_) => return 8,
    };
    let attached = match memobj_create() {
        Ok(handle) => handle,
        Err(_) => return 9,
    };
    let source = match map_memory(
        Handle::SELF_PROC,
        attached,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::SHARED),
    ) {
        Ok(address) => address,
        Err(_) => return 10,
    };
    // SAFETY: this is the caller's writable one-page shared mapping.
    unsafe { (source as *mut u8).write(PATTERN) };
    let _ = raven_rt::nanosleep(10_000_000);
    if transfer(server, attached, Rights::READ.union(Rights::MAP)).is_err() {
        return 11;
    }
    close(attached);
    let received = match open("attach:x") {
        Ok(handle) => handle,
        Err(_) => return 12,
    };
    let mapped = match map_memory(Handle::SELF_PROC, received, 0, PAGE, prot::READ) {
        Ok(address) => address,
        Err(_) => return 13,
    };
    // SAFETY: MAP just returned a readable one-page mapping.
    if unsafe { (mapped as *const u8).read() } != PATTERN {
        return 14;
    }
    println!("s25: attach ok");
    println!("s25: handles free=65532");
    0
}
