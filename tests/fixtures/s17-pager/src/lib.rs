// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S17 pager-fixture helpers: raw syscalls, the synthetic page pattern and the
//! `PagerPacket` codec re-used from the host-testable crate.

#![no_std]
#![allow(unsafe_code)] // Raw syscall wrappers; every block carries a SAFETY comment.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map};
use raven_ipc::protocol::{PACKET_SIZE, SchemePacket};
use raven_rt::sys::{syscall1, syscall2, syscall3, syscall6};

/// Page size.
pub const PAGE: usize = 4096;
/// Open flag: read.
pub const O_READ: u32 = 1;
/// Open flag: write.
pub const O_WRITE: u32 = 2;
/// Open flag: read and write.
pub const O_RDWR: u32 = O_READ | O_WRITE;

/// Convert a raw syscall result.
pub fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

/// Convert a raw handle result.
pub fn handle(raw: isize) -> Result<Handle, Errno> {
    u32::try_from(result(raw)?)
        .map(Handle)
        .map_err(|_| Errno::EINVAL)
}

/// Terminate the calling process.
pub fn exit(code: i32) -> ! {
    let _ = result(unsafe { syscall1(Syscall::EXIT as usize, code as usize) });
    loop {
        core::hint::spin_loop();
    }
}

/// Print a failure line and exit with status 1.
pub fn fail(message: &str) -> ! {
    raven_rt::println!("s17: failed {message}");
    exit(1)
}

/// Sleep for `ns` nanoseconds.
pub fn nanosleep(ns: u64) -> Result<(), Errno> {
    result(unsafe { syscall1(Syscall::NANOSLEEP as usize, ns as usize) }).map(|_| ())
}

/// Close one handle.
pub fn handle_close(handle: Handle) -> Result<(), Errno> {
    result(unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) }).map(|_| ())
}

/// Register a scheme name for this process.
pub fn scheme_register(name: &str) -> Result<(), Errno> {
    result(unsafe {
        syscall3(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
        )
    })
    .map(|_| ())
}

/// Open a scheme path.
pub fn open(path: &str, flags: u32) -> Result<Handle, Errno> {
    // SAFETY: the kernel validates the path pointer/length and the flags.
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

/// Receive one scheme request packet.
pub fn recv(packet: &mut [u8; PACKET_SIZE]) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall2(
            Syscall::RECV as usize,
            packet.as_mut_ptr() as usize,
            packet.len(),
        )
    })
}

/// Send one scheme reply packet.
pub fn send(request: Handle, packet: &[u8; PACKET_SIZE]) -> Result<(), Errno> {
    result(unsafe {
        syscall3(
            Syscall::SEND as usize,
            request.0 as usize,
            packet.as_ptr() as usize,
            packet.len(),
        )
    })
    .map(|_| ())
}

/// One typed `CALL` with an optional response buffer.
pub fn call(handle: Handle, request: &[u8], response: Option<&mut [u8]>) -> Result<usize, Errno> {
    let (response_ptr, response_len) = match response {
        Some(buffer) => (buffer.as_mut_ptr() as usize, buffer.len()),
        None => (0, 0),
    };
    result(unsafe {
        syscall6(
            Syscall::CALL as usize,
            handle.0 as usize,
            request.as_ptr() as usize,
            request.len(),
            response_ptr,
            response_len,
            0,
        )
    })
}

/// Wait for a process and return its exit code.
pub fn proc_wait(process: Handle) -> Result<i32, Errno> {
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

/// Create a memory object; `flags` carries `memobj::PAGER` for a pager-backed object.
pub fn memobj_create(len: usize, flags: usize) -> Result<Handle, Errno> {
    handle(unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, len, flags) })
}

/// Supply one page to a pager-backed object.
pub fn pager_supply(object: Handle, offset: usize, data: &[u8]) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::PAGER_SUPPLY as usize,
            object.0 as usize,
            offset,
            data.as_ptr() as usize,
            data.len(),
            0,
            0,
        )
    })
    .map(|_| ())
}

/// Map `len` bytes of `object` at a kernel-chosen address.
pub fn map_object(
    object: Handle,
    offset: usize,
    len: usize,
    protection: usize,
    shared: bool,
) -> Result<usize, Errno> {
    let flags = map::pack(protection, if shared { map::SHARED } else { 0 });
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            offset,
            0,
            len,
            flags,
        )
    })
}

/// Unmap a range owned by this process.
pub fn unmap(address: usize, len: usize) -> Result<(), Errno> {
    result(unsafe {
        syscall3(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            len,
        )
    })
    .map(|_| ())
}

/// The synthetic file content for one page.
pub fn page_pattern(page: u64, output: &mut [u8]) {
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = (page as u8)
            .wrapping_mul(37)
            .wrapping_add((index as u8).wrapping_mul(11))
            .wrapping_add(0x5a);
    }
}

/// Encode a reply packet.
pub fn encode_reply(packet: &SchemePacket) -> Result<[u8; PACKET_SIZE], Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    raven_ipc::protocol::encode(packet, &mut bytes).map_err(|_| Errno::EPROTO)?;
    Ok(bytes)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
