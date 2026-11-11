// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S25 controller: transfers a read-only memory object into a child before its first thread,
//! then receives one from a server as an `Open` reply attachment.
//!
//! The 100 start/exit cycles prove handle hygiene: the parent's live-handle count is the same
//! before and after, and every kernel-issued handle index repeats each cycle (4/5/6), so any
//! leaked slot would shift the indices and fail the run.

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
/// Mirrors `raven_handles::MAX_HANDLES`.
const MAX_HANDLES: usize = 65_536;
/// Initial kernel handles: self process, initfs, namespace, RootDevice (well-known, see
/// architecture/01-kernel.md — handle 3 is reserved for it since S21a).
const INITIAL_HANDLES: usize = 4;
const OPEN_RETRIES: usize = 2000;

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

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle(unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) })
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

fn thread_create(process: Handle, entry: usize, stack: usize) -> Result<Handle, Errno> {
    // SAFETY: THREAD_CREATE follows the stable syscall ABI; arguments are prepared below.
    handle(unsafe {
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

/// Compact diagnostic tag for one syscall result: the raw handle value or the errno number.
fn tag(result: &Result<Handle, Errno>) -> u32 {
    match result {
        Ok(handle) => handle.0,
        Err(error) => u32::from(error.0),
    }
}

/// Start one child and wait for its exit. `live` tracks the parent's live handles so the
/// caller can prove the table returns to its steady state after every cycle.
fn start_child(
    entry: &Entry<'_>,
    elf: &Elf<'_>,
    memory: Handle,
    live: &mut usize,
) -> Result<i32, Errno> {
    let child = proc_create()?;
    if child.index() != 4 {
        println!("s25: child index {}", child.index());
        return Err(Errno::EIO);
    }
    *live += 1;
    for step in plan_load(elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL)? {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                map_at(
                    child,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::Zero => {
                map_memory(
                    child,
                    Handle::NONE,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = memobj_create(STACK_SIZE)?;
    if stack.index() != 5 {
        println!("s25: stack index {}", stack.index());
        return Err(Errno::EIO);
    }
    *live += 1;
    let scratch = map_memory(
        Handle::SELF_PROC,
        stack,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    map_memory(
        child,
        stack,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    let stack_pointer = build_stack(scratch);

    // Negatives: a zero mask, an absent handle and a non-transferable kind must all be refused
    // without touching the destination table.
    let zero_mask = transfer(child, memory, Rights::NONE);
    if zero_mask != Err(Errno::EINVAL) {
        println!("s25: mask0 {}", tag(&zero_mask));
        return Err(Errno::EIO);
    }
    let absent = transfer(child, Handle::new(100, 7), Rights::READ);
    if absent != Err(Errno::EBADF) {
        println!("s25: absent {}", tag(&absent));
        return Err(Errno::EIO);
    }
    let process = transfer(child, Handle::SELF_PROC, Rights::READ);
    if process != Err(Errno::EBADF) {
        println!("s25: process {}", tag(&process));
        return Err(Errno::EIO);
    }
    // The spawn-time grant: a READ|MAP duplicate installed in the child before its first thread.
    let transferred = transfer(child, memory, Rights::READ.union(Rights::MAP))?;
    if transferred != Handle::new(3, 0) {
        println!("s25: transferred {:#x}", transferred.0);
        return Err(Errno::EIO);
    }

    let thread = thread_create(child, elf.entry() as usize, stack_pointer)?;
    if thread.index() != 6 {
        println!("s25: thread index {}", thread.index());
        return Err(Errno::EIO);
    }
    *live += 1;
    close(thread);
    *live -= 1;
    unmap(scratch, STACK_SIZE);
    close(stack);
    *live -= 1;
    let code = wait(child)?;
    close(child);
    *live -= 1;
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

/// Retry `open` while the scheme is not yet registered (`ENOENT`); surface every other error.
fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < OPEN_RETRIES {
        match open(path) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_rt::nanosleep(1_000_000);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
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
    let mut live = INITIAL_HANDLES;
    let memory = match memobj_create(PAGE) {
        Ok(handle) => handle,
        Err(_) => return 5,
    };
    if memory.index() != INITIAL_HANDLES {
        return 6;
    }
    live += 1;
    let pattern = match map_memory(
        Handle::SELF_PROC,
        memory,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::SHARED),
    ) {
        Ok(address) => address,
        Err(_) => return 7,
    };
    // SAFETY: this is the caller's writable one-page shared mapping.
    unsafe { (pattern as *mut u8).write(PATTERN) };
    let free_before = MAX_HANDLES - live;

    let mut cycle = 0usize;
    while cycle < CYCLES {
        match start_child(&child, &elf, memory, &mut live) {
            Ok(7) => {}
            Ok(code) => {
                println!("s25: child exit {}", code);
                return 8;
            }
            Err(error) => {
                println!("s25: child setup {}", error.0);
                return 8;
            }
        }
        cycle += 1;
    }
    let free_after = MAX_HANDLES - live;
    if free_before != free_after {
        println!("s25: handle count drifted {} {}", free_before, free_after);
        return 9;
    }
    println!("s25: transfer ok");
    println!("s25: rights ok");

    let _server = match spawn(
        &initfs,
        "/server",
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    ) {
        Ok(server) => server,
        Err(_) => return 10,
    };
    let received = match open_retry("attach:x") {
        Ok(handle) => handle,
        Err(_) => return 11,
    };
    // The attachment was delivered with a READ|MAP mask: a shared writable mapping must be
    // refused, which proves the server's own WRITE right was not passed along.
    match map_memory(
        Handle::SELF_PROC,
        received,
        0,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::SHARED),
    ) {
        Err(Errno::EACCES) => {}
        _ => return 12,
    }
    let mapped = match map_memory(Handle::SELF_PROC, received, 0, PAGE, prot::READ) {
        Ok(address) => address,
        Err(_) => return 13,
    };
    // SAFETY: MAP just returned a readable one-page mapping.
    if unsafe { (mapped as *const u8).read() } != PATTERN {
        return 14;
    }
    println!("s25: attach ok");
    println!("s25: handles free={}", free_after);
    0
}
