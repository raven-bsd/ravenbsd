// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S73 fixture helpers: cross-architecture raw syscalls and a staged child spawn.
//!
//! A child is created with `PROC_CREATE`, its ELF and stack are mapped, the caller installs a
//! reduced-rights capability with `HANDLE_TRANSFER`, and only then is its first thread started.
//! That ordering makes the transfer-before-first-instruction and revoke-before-next-use cases
//! deterministic. The raw syscalls delegate to `raven_rt::sys` (x86-64 `syscall`, aarch64 `svc`).

#![no_std]
#![allow(unsafe_code)] // Raw syscall arguments and the shared stack; each block has SAFETY.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;

#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64 as layout;
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64 as layout;

/// Page size.
pub const PAGE: usize = 4096;
/// Fixed address where the controller maps the initfs.
pub const INITFS_BASE: usize = 0x1000_0000;
/// Scratch window for building a child stack.
pub const STACK_SCRATCH: usize = 0x2000_0000;
/// Child stack size.
pub const STACK_SIZE: usize = 64 * 1024;
/// Highest stack address.
pub const STACK_TOP: usize = (layout::USER_END as usize) - PAGE;
/// Lowest stack address.
pub const STACK_BASE: usize = STACK_TOP - STACK_SIZE;

/// A child that is mapped but whose first thread has not started.
pub struct Staged {
    /// Child process handle.
    pub process: Handle,
    /// ELF entry address.
    pub entry: usize,
    /// Initial stack pointer.
    pub stack: usize,
}

#[cfg(target_arch = "x86_64")]
fn machine() -> Machine {
    Machine::X86_64
}

#[cfg(target_arch = "aarch64")]
fn machine() -> Machine {
    Machine::Aarch64
}

#[cfg(target_arch = "riscv64")]
fn machine() -> Machine {
    Machine::Riscv64
}

fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { raven_rt::sys::syscall1(number, a0) }
}

fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { raven_rt::sys::syscall2(number, a0, a1) }
}

fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { raven_rt::sys::syscall3(number, a0, a1, a2) }
}

fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { raven_rt::sys::syscall6(number, a0, a1, a2, a3, a4, a5) }
}

/// Decode a `-errno` result.
pub fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Create an empty process.
pub fn proc_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::PROC_CREATE as usize, 0))
}

/// Create an anonymous memory object.
pub fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
}

/// Map memory into a process.
#[allow(clippy::too_many_arguments)] // Mirrors the six-register MAP syscall.
pub fn map_memory(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    result(syscall6(
        Syscall::MAP as usize,
        process.0 as usize,
        object.0 as usize,
        offset,
        address,
        len,
        flags,
    ))
}

/// Unmap memory in a process.
pub fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::UNMAP as usize,
        process.0 as usize,
        address,
        len,
    ))
    .map(|_| ())
}

/// Duplicate a handle with a reduced rights mask.
pub fn dup(source: Handle, mask: Rights) -> Result<Handle, Errno> {
    handle(syscall2(
        Syscall::HANDLE_DUP as usize,
        source.0 as usize,
        mask.0 as usize,
    ))
}

/// Revoke a rights mask from a handle and its descendants.
pub fn revoke(handle: Handle, mask: Rights) -> Result<(), Errno> {
    result(syscall2(
        Syscall::HANDLE_REVOKE as usize,
        handle.0 as usize,
        mask.0 as usize,
    ))
    .map(|_| ())
}

/// Transfer a handle to `process` with a reduced rights mask.
pub fn transfer(process: Handle, object: Handle, mask: Rights) -> Result<Handle, Errno> {
    handle(syscall3(
        Syscall::HANDLE_TRANSFER as usize,
        process.0 as usize,
        object.0 as usize,
        mask.0 as usize,
    ))
}

/// Fork `source` into a new process that shares the handle lineage nodes.
///
/// `PROC_FORK` is exposed as raw number `0x39` by the kernel until the ABI crate adds a variant.
pub fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    handle(syscall2(0x39, source.0 as usize, 0))
}

/// Create a thread in `process`.
pub fn thread_create(
    process: Handle,
    entry: usize,
    stack: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    handle(syscall6(
        Syscall::THREAD_CREATE as usize,
        process.0 as usize,
        entry,
        stack,
        arg,
        0,
        0,
    ))
}

/// Wait for a process and return its exit code.
pub fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    result(syscall2(
        Syscall::PROC_WAIT as usize,
        process.0 as usize,
        &mut code as *mut i32 as usize,
    ))?;
    Ok(code)
}

/// Close a handle.
pub fn handle_close(handle: Handle) {
    let _ = result(syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize));
}

/// Sleep for `ns` nanoseconds.
pub fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

/// Create and map a child without starting its first thread.
pub fn stage(initfs: &Initfs<'_>, path: &str) -> Result<Staged, Errno> {
    let entry = initfs.get(path).ok_or(Errno::ENOENT)?;
    let elf = Elf::parse(entry.data, machine(), layout::USER_MIN..layout::USER_END)
        .map_err(|_| Errno::EINVAL)?;
    let child = proc_create()?;
    for step in plan_load(&elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL)? {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                map_memory(
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
                    0,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = memobj_create(STACK_SIZE)?;
    let scratch = map_memory(
        Handle::SELF_PROC,
        stack,
        0,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    // The child stack is private: a `PROC_FORK` clone must get its own COW pages, not share the
    // controller-built stack with the original thread.
    map_memory(
        child,
        stack,
        0,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    )?;
    let stack_pointer = build_stack(scratch);
    unmap(Handle::SELF_PROC, STACK_SCRATCH, STACK_SIZE)?;
    handle_close(stack);
    Ok(Staged {
        process: child,
        entry: elf.entry() as usize,
        stack: stack_pointer,
    })
}

/// Start a staged child's first thread and close the thread handle.
pub fn start(staged: &Staged) -> Result<Handle, Errno> {
    let thread = thread_create(staged.process, staged.entry, staged.stack, 0)?;
    handle_close(thread);
    Ok(staged.process)
}

fn build_stack(base: usize) -> usize {
    let stack = STACK_TOP - 0x100;
    let mut offset = stack - STACK_BASE;
    // argc=1, argv[0], argv null, env null, AT_PAGESZ, 4096, auxv terminator.
    for value in [1u64, (STACK_TOP - 0x40) as u64, 0, 0, 6, PAGE as u64, 0, 0] {
        // SAFETY: the shared stack mapping is writable and `offset` stays inside its fixed size.
        unsafe { ((base + offset) as *mut u64).write_unaligned(value.to_le()) };
        offset = offset.saturating_add(8);
    }
    // SAFETY: the string and its terminator are inside the writable shared stack mapping.
    unsafe {
        ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'x');
        ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0);
    }
    stack
}
