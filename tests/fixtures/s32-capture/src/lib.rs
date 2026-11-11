// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S32 capture-lifetime fixture helpers: raw scheme syscalls, mapping and threads.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly and mapped buffers; every block has SAFETY.

extern crate alloc;

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_ipc::protocol::{self, PACKET_SIZE, SchemePacket};

/// Page size.
pub const PAGE: usize = 4096;
/// Fixed address where the fixture maps the initfs in the controller.
pub const INITFS_BASE: usize = 0x1000_0000;
/// Open flag: read.
pub const O_READ: u32 = 1;
/// Open flag: write.
pub const O_WRITE: u32 = 2;
/// Open flag: read and write.
pub const O_RDWR: u32 = O_READ | O_WRITE;

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

/// Create a thread in `SELF_PROC` starting at `entry` with `stack_top` and `arg`.
pub fn thread_create(entry: usize, stack_top: usize, arg: usize) -> Result<Handle, Errno> {
    let handle = result(syscall5(
        Syscall::THREAD_CREATE as usize,
        Handle::SELF_PROC.0 as usize,
        entry,
        stack_top,
        arg,
        0,
    ))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Close a handle without a scheme notification.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize)).map(|_| ())
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

/// Open a scheme path.
pub fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
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

/// Receive one request packet.
pub fn recv(packet: &mut [u8]) -> Result<Handle, Errno> {
    let handle = result(syscall2(
        Syscall::RECV as usize,
        packet.as_mut_ptr() as usize,
        packet.len(),
    ))?;
    u32::try_from(handle).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Send one response packet.
pub fn send(request: Handle, packet: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SEND as usize,
        request.0 as usize,
        packet.as_ptr() as usize,
        packet.len(),
    ))
    .map(|_| ())
}

/// Close a scheme file.
pub fn close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::CLOSE as usize, handle.0 as usize)).map(|_| ())
}

/// Typed call with an optional response buffer.
pub fn call(handle: Handle, request: &[u8], response: Option<&mut [u8]>) -> Result<usize, Errno> {
    let (response_ptr, response_len) = match response {
        Some(buffer) => (buffer.as_mut_ptr() as usize, buffer.len()),
        None => (0, 0),
    };
    result(syscall6(
        Syscall::CALL as usize,
        handle.0 as usize,
        request.as_ptr() as usize,
        request.len(),
        response_ptr,
        response_len,
        0,
    ))
}

/// Map anonymous memory anywhere.
pub fn map_anonymous(len: usize) -> Result<*mut u8, Errno> {
    map_hint(0, len)
}

/// Map anonymous memory with an address hint.
pub fn map_hint(hint: usize, len: usize) -> Result<*mut u8, Errno> {
    let address = result(syscall6(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::NONE.0 as usize,
        0,
        hint,
        len,
        map::pack(prot::READ | prot::WRITE, 0),
    ))?;
    Ok(address as *mut u8)
}

/// Map anonymous memory exactly at `address`.
pub fn map_fixed(address: usize, len: usize) -> Result<*mut u8, Errno> {
    let mapped = result(syscall6(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::NONE.0 as usize,
        0,
        address,
        len,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    ))?;
    Ok(mapped as *mut u8)
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

/// Change protections of a range in this process.
pub fn protect(address: usize, len: usize, protection: usize) -> Result<(), Errno> {
    result(syscall6(
        Syscall::PROTECT as usize,
        Handle::SELF_PROC.0 as usize,
        address,
        len,
        protection,
        0,
        0,
    ))
    .map(|_| ())
}

/// Map the initfs image in the controller.
pub fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = result(syscall6(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::INITFS.0 as usize,
        0,
        0,
        PAGE,
        prot::READ,
    ))?;
    let total = {
        // SAFETY: the mapping is one readable page; the C7 header puts total_size at byte 40.
        unsafe { ((page + 40) as *const u64).read_unaligned() as usize }
    };
    let _ = result(syscall3(
        Syscall::UNMAP as usize,
        Handle::SELF_PROC.0 as usize,
        page,
        PAGE,
    ));
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = result(syscall6(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::INITFS.0 as usize,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    ))?;
    // SAFETY: the loader mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

/// Encode a response packet into a fixed buffer.
pub fn encode_packet(packet: &SchemePacket) -> Result<[u8; PACKET_SIZE], Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(packet, &mut bytes).map_err(|_| Errno::EPROTO)?;
    Ok(bytes)
}

/// Decode a request packet.
pub fn decode_packet(bytes: &[u8]) -> Result<SchemePacket, Errno> {
    protocol::decode(bytes).map_err(|_| Errno::EPROTO)
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

/// Fill a mapped buffer with `value`.
///
/// # Safety
///
/// `address..address + len` must be a writable mapping of this process.
pub unsafe fn fill(address: usize, len: usize, value: u8) {
    // SAFETY: the caller guarantees a writable mapping of `len` bytes.
    unsafe {
        core::ptr::write_bytes(address as *mut u8, value, len);
    }
}

/// True when every byte in `address..address + len` equals `value`.
///
/// # Safety
///
/// `address..address + len` must be a readable mapping of this process.
pub unsafe fn all_equal(address: usize, len: usize, value: u8) -> bool {
    let mut index = 0usize;
    while index < len {
        // SAFETY: the caller guarantees a readable mapping of `len` bytes.
        if unsafe { *(address as *const u8).add(index) } != value {
            return false;
        }
        index = index.saturating_add(1);
    }
    true
}

/// True when `address..address + len` matches `(index as u8) ^ seed`.
///
/// # Safety
///
/// `address..address + len` must be a readable mapping of this process.
pub unsafe fn pattern_matches(address: usize, len: usize, seed: u8) -> bool {
    let mut index = 0usize;
    while index < len {
        let expected = (index as u8) ^ seed;
        // SAFETY: the caller guarantees a readable mapping of `len` bytes.
        if unsafe { *(address as *const u8).add(index) } != expected {
            return false;
        }
        index = index.saturating_add(1);
    }
    true
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
