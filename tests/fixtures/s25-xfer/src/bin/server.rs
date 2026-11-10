// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S25 attachment server: answers `Open` by attaching its own pattern memory object.
//!
//! The registration handle lands at index 3 and the memory object at index 4 (the table starts
//! empty and this process is single-threaded), so both positions are asserted before serving.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_ipc::protocol::{Attachment, Opcode, PACKET_SIZE, REPLY_HANDLE, decode, encode};
use raven_rt::entry;
use raven_rt::sys::{syscall2, syscall3, syscall6};
use raven_s25_xfer as _;

entry!(main);

const PAGE: usize = 4096;
const PATTERN: u8 = 0xa5;
/// Handle of this server's own memory object: initial 0..2 plus the registration at 3.
const MEMORY: Handle = Handle::new(4, 0);

fn result(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

fn handle(value: isize) -> Result<Handle, Errno> {
    u32::try_from(result(value)?)
        .map(Handle)
        .map_err(|_| Errno::EINVAL)
}

fn scheme_register() -> Result<Handle, Errno> {
    let name = b"attach";
    handle(unsafe {
        syscall3(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
        )
    })
}

fn memobj_create() -> Result<Handle, Errno> {
    handle(unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, PAGE, 0) })
}

fn map_memory(object: Handle, flags: usize) -> Result<usize, Errno> {
    // SAFETY: MAP follows the stable syscall ABI; the kernel validates every argument.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            0,
            PAGE,
            flags,
        )
    })
}

fn unmap(address: usize, length: usize) {
    // SAFETY: UNMAP takes scalar arguments; this helper unmaps only this server's scratch page.
    let _ = unsafe {
        syscall3(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            length,
        )
    };
}

fn recv(packet: &mut [u8; PACKET_SIZE]) -> Result<Handle, Errno> {
    let value = result(unsafe {
        syscall2(
            Syscall::RECV as usize,
            packet.as_mut_ptr() as usize,
            packet.len(),
        )
    })?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn send(request: Handle, packet: &[u8; PACKET_SIZE]) -> Result<(), Errno> {
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

/// Write the pattern into the object and read it back through a private mapping.
fn fill_pattern(object: Handle) -> Result<(), Errno> {
    let address = map_memory(object, map::pack(prot::READ | prot::WRITE, map::SHARED))?;
    // SAFETY: the mapping is this server's writable one-page shared view of the object.
    unsafe { (address as *mut u8).write(PATTERN) };
    // SAFETY: the same readable mapping, one byte inside it.
    let check = unsafe { (address as *const u8).read() };
    unmap(address, PAGE);
    if check == PATTERN {
        Ok(())
    } else {
        Err(Errno::EIO)
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let registration = match scheme_register() {
        Ok(registration) => registration,
        Err(_) => return 1,
    };
    if registration.index() != 3 {
        return 2;
    }
    let memory = match memobj_create() {
        Ok(memory) => memory,
        Err(_) => return 3,
    };
    if memory != MEMORY {
        return 4;
    }
    if fill_pattern(memory).is_err() {
        return 5;
    }
    let mut bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match recv(&mut bytes) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut reply = match decode(&bytes) {
            Ok(packet) => packet,
            Err(_) => return 6,
        };
        if reply.opcode != Opcode::Open.as_u16() {
            reply.status = i32::from(Errno::EPROTO.0);
        } else {
            reply.status = 0;
            reply.flags |= REPLY_HANDLE;
            reply.attach_count = 1;
            reply.attach[0] = Attachment {
                handle: MEMORY.0,
                rights_mask: Rights::READ.union(Rights::MAP).0,
            };
        }
        let mut response = [0u8; PACKET_SIZE];
        if encode(&reply, &mut response).is_err() || send(request, &response).is_err() {
            return 7;
        }
    }
}
