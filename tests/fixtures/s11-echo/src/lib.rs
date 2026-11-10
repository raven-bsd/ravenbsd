// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S11 echo-fixture helpers: raw scheme syscalls and initfs mapping.

#![no_std]
#![allow(unsafe_code)] // Raw syscall assembly and the mapped initfs image; every block has SAFETY.

extern crate alloc;

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};

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

/// Sleep for `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    result(syscall1(Syscall::NANOSLEEP as usize, ns as usize)).map(|_| ())
}

/// Debug-console write.
pub fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    result(syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    ))
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

/// Close a scheme file.
pub fn close(handle: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::CLOSE as usize, handle.0 as usize)).map(|_| ())
}

/// Receive one request packet; returns the server-side request handle.
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

/// Map anonymous memory.
pub fn map_anonymous(len: usize) -> Result<*mut u8, Errno> {
    let address = result(syscall6(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::NONE.0 as usize,
        0,
        0,
        len,
        map::pack(prot::READ | prot::WRITE, 0),
    ))?;
    Ok(address as *mut u8)
}

/// Map a page of the initfs at a fixed address.
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

/// Build an `Open`-style request.
pub fn request_packet(opcode: Opcode, request_id: u64, file_id: u64) -> SchemePacket {
    SchemePacket::request(opcode, request_id, file_id)
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

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
