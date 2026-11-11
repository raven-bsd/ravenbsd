// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S37 filesystem-plumbing fixture helpers: raw scheme, mapping and process syscalls.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly and mapped buffers; every block has SAFETY.

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};

/// Page size.
pub const PAGE: usize = 4096;
/// Fixed address where the fixture maps the initfs image.
pub const INITFS_BASE: usize = 0x1000_0000;
/// Stack A maps for the death reader.
pub const DEATH_STACK_BASE: usize = 0x2000_0000;
/// Death-reader stack size.
pub const DEATH_STACK_SIZE: usize = 64 * 1024;

/// Raw `syscall` instruction.
#[inline(always)]
pub fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: the caller passes valid registers for the S11 ABI; the kernel validates every
    // pointer and handle.
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

/// One-argument syscall.
pub fn syscall1(number: usize, a0: usize) -> isize {
    syscall6(number, a0, 0, 0, 0, 0, 0)
}

/// Two-argument syscall.
pub fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    syscall6(number, a0, a1, 0, 0, 0, 0)
}

/// Three-argument syscall.
pub fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    syscall6(number, a0, a1, a2, 0, 0, 0)
}

/// Five-argument syscall.
pub fn syscall5(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    syscall6(number, a0, a1, a2, a3, a4, 0)
}

/// Convert a raw return value.
pub fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

/// Exit the process.
pub fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

/// Exit the calling thread.
pub fn thread_exit(code: i32) -> ! {
    let _ = syscall1(Syscall::THREAD_EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

/// Sleep for `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    result(syscall1(Syscall::NANOSLEEP as usize, ns as usize)).map(|_| ())
}

/// Write bytes to the debug console.
pub fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    result(syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    ))
}

/// Print a fixed line (the death reader has no formatting runtime).
pub fn print_line(text: &str) {
    let _ = debug_write(text.as_bytes());
    let _ = debug_write(b"\n");
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

/// Open a scheme path with raw K66 wire flags.
pub fn open(path: &str, flags: u16) -> Result<Handle, Errno> {
    let handle = result(syscall6(
        Syscall::OPEN as usize,
        path.as_ptr() as usize,
        path.len(),
        flags as usize,
        0,
        0,
        0,
    ))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
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

/// Write to a scheme file.
pub fn write(handle: Handle, buffer: &[u8]) -> Result<usize, Errno> {
    result(syscall3(
        Syscall::WRITE as usize,
        handle.0 as usize,
        buffer.as_ptr() as usize,
        buffer.len(),
    ))
}

/// Close a scheme file.
pub fn close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::CLOSE as usize, handle.0 as usize)).map(|_| ())
}

/// Receive one scheme request packet; returns the server-side request handle.
pub fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    let handle = result(syscall2(
        Syscall::RECV as usize,
        packet.as_mut_ptr() as usize,
        packet.len(),
    ))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Send one scheme response packet.
pub fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SEND as usize,
        request.0 as usize,
        packet.as_ptr() as usize,
        packet.len(),
    ))
    .map(|_| ())
}

/// Duplicate a scheme-file handle.
pub fn dup(handle: Handle, mask: u32) -> Result<Handle, Errno> {
    let duplicate = result(syscall2(
        Syscall::DUP as usize,
        handle.0 as usize,
        mask as usize,
    ))?;
    u32::try_from(duplicate)
        .map(Handle)
        .map_err(|_| Errno::EINVAL)
}

/// Read the 64-byte stat record for a handle.
pub fn fstat(handle: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::FSTAT as usize,
        handle.0 as usize,
        out.as_mut_ptr() as usize,
        out.len(),
    ))
    .map(|_| ())
}

/// Typed call with a request and response buffer.
pub fn call(handle: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
    result(syscall6(
        Syscall::CALL as usize,
        handle.0 as usize,
        request.as_ptr() as usize,
        request.len(),
        response.as_mut_ptr() as usize,
        response.len(),
        0,
    ))
}

/// Map a memory object or fresh memory into `process`.
pub fn map(
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

/// Unmap a range in this process.
pub fn unmap(address: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::UNMAP as usize,
        Handle::SELF_PROC.0 as usize,
        address,
        len,
    ))
    .map(|_| ())
}

/// Create an empty process.
pub fn proc_create() -> Result<Handle, Errno> {
    let handle = result(syscall1(Syscall::PROC_CREATE as usize, 0))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Create a zero-filled memory object of `len` bytes.
pub fn memobj_create(len: usize) -> Result<Handle, Errno> {
    let handle = result(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
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

/// Transfer a handle into `dest_process`.
pub fn handle_transfer(dest_process: Handle, object: Handle, rights: u32) -> Result<Handle, Errno> {
    let handle = result(syscall3(
        Syscall::HANDLE_TRANSFER as usize,
        dest_process.0 as usize,
        object.0 as usize,
        rights as usize,
    ))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Start a thread in `process`.
pub fn thread_create(
    process: Handle,
    entry: usize,
    stack: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    let handle = result(syscall5(
        Syscall::THREAD_CREATE as usize,
        process.0 as usize,
        entry,
        stack,
        arg,
        0,
    ))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Close a kernel handle without a scheme notification.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize)).map(|_| ())
}

/// Map the whole initfs image from `handle` at [`INITFS_BASE`].
pub fn map_initfs(handle: Handle) -> Result<&'static [u8], Errno> {
    let page = map(
        Handle::SELF_PROC,
        handle,
        0,
        0,
        PAGE,
        map::pack(prot::READ, 0),
    )?;
    let total = {
        // SAFETY: the one-page mapping contains the C7 total-size field at byte 40.
        unsafe { ((page + 40) as *const u64).read_unaligned() as usize }
    };
    let _ = unmap(page, PAGE);
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map(
        Handle::SELF_PROC,
        handle,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the kernel mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
