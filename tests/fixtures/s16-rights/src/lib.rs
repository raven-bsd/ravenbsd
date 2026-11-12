// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S16 fixture helpers: raw syscalls and a staged child spawn.
//!
//! A child is created with `PROC_CREATE`, its ELF and stack are mapped, the caller may install a
//! capability or namespace grant, and only then is its first thread started. That ordering makes
//! the revoke-before-first-instruction and prefix-before-server cases deterministic.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly, initfs mapping and the shared stack; each block has SAFETY.

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;

/// Page size.
pub const PAGE: usize = 4096;
/// Fixed address where the controller maps the initfs.
pub const INITFS_BASE: usize = 0x1000_0000;
/// Scratch window for building a child stack.
pub const STACK_SCRATCH: usize = 0x2000_0000;
/// Child stack size.
pub const STACK_SIZE: usize = 64 * 1024;
/// Highest stack address.
pub const STACK_TOP: usize = (USER_END as usize) - PAGE;
/// Lowest stack address.
pub const STACK_BASE: usize = STACK_TOP - STACK_SIZE;
/// Open for reading.
pub const O_READ: u32 = 1;
/// Open for reading and writing.
pub const O_RDWR: u32 = 3;

/// A child that is mapped but whose first thread has not started.
pub struct Staged {
    /// Child process handle.
    pub process: Handle,
    /// ELF entry address.
    pub entry: usize,
    /// Initial stack pointer.
    pub stack: usize,
}

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
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates every
    // pointer and handle. `r10` is caller-saved under the syscall convention.
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

fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
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

/// Add, remove or clear a namespace prefix.
pub fn namespace(process: Handle, op: u32, prefix: &[u8]) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::PROC_NAMESPACE as usize,
            process.0 as usize,
            op as usize,
            prefix.as_ptr() as usize,
            prefix.len(),
            0,
            0,
        )
    })
    .map(|_| ())
}

/// Create a thread in `process`.
pub fn thread_create(
    process: Handle,
    entry: usize,
    stack: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack,
            arg,
            0,
            0,
        )
    })
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

/// Terminate a process with `code`.
pub fn proc_terminate(process: Handle, code: i32) -> Result<(), Errno> {
    result(syscall2(
        Syscall::PROC_TERMINATE as usize,
        process.0 as usize,
        code as usize,
    ))
    .map(|_| ())
}

/// Close a handle.
pub fn handle_close(handle: Handle) {
    let _ = result(syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize));
}

/// Open a scheme path.
pub fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::OPEN as usize,
            path.as_ptr() as usize,
            path.len(),
            flags as usize,
            0,
            0,
            0,
        )
    })
}

/// Read from a scheme file.
pub fn read(handle: Handle, buffer: &mut [u8]) -> Result<usize, Errno> {
    result(syscall3(
        Syscall::READ as usize,
        handle.0 as usize,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
    ))
}

/// Close a scheme file.
pub fn close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::CLOSE as usize, handle.0 as usize)).map(|_| ())
}

/// Register a scheme name for the calling process.
pub fn scheme_register(name: &str) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SCHEME_REGISTER as usize,
        name.as_ptr() as usize,
        name.len(),
        Handle::SELF_PROC.0 as usize,
    ))
    .map(|_| ())
}

/// Receive one server request.
pub fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    handle(syscall2(
        Syscall::RECV as usize,
        packet.as_mut_ptr() as usize,
        packet.len(),
    ))
}

/// Send one server reply.
pub fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SEND as usize,
        request.0 as usize,
        packet.as_ptr() as usize,
        packet.len(),
    ))
    .map(|_| ())
}

/// Sleep for `ns` nanoseconds.
pub fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

/// Map the boot initfs read-only and return its bytes.
pub fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ as usize,
    )?;
    // SAFETY: the mapping is one readable page; the initfs header stores total_size at byte 40.
    let total = unsafe { ((page + 40) as *const u64).read_unaligned() as usize };
    let _ = unmap(Handle::SELF_PROC, page, PAGE);
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the kernel mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

/// Create and map a child without starting its first thread.
pub fn stage(initfs: &Initfs<'_>, path: &str) -> Result<Staged, Errno> {
    let entry = initfs.get(path).ok_or(Errno::ENOENT)?;
    let elf =
        Elf::parse(entry.data, Machine::X86_64, USER_MIN..USER_END).map_err(|_| Errno::EINVAL)?;
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
    map_memory(
        child,
        stack,
        0,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
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

/// Stage and start a child with no grants.
pub fn spawn(initfs: &Initfs<'_>, path: &str) -> Result<Handle, Errno> {
    let staged = stage(initfs, path)?;
    start(&staged)
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

/// Read a byte through a scheme handle.
pub fn read_byte(handle: Handle) -> Result<u8, Errno> {
    let mut byte = [0u8; 1];
    let count = read(handle, &mut byte)?;
    if count != 1 {
        return Err(Errno::EIO);
    }
    Ok(byte[0])
}
