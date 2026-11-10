// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S25 attachment server: returns the memory object transferred into slot four on Open.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_ipc::protocol::{Attachment, Opcode, PACKET_SIZE, REPLY_HANDLE, encode};
use raven_rt::entry;
use raven_rt::sys::{syscall2, syscall3};
use raven_s25_xfer as _;

entry!(main);

const TRANSFERRED: Handle = Handle::new(4, 0);

fn result(value: isize) -> Result<usize, Errno> {
    Errno::from_ret(value)
}

fn scheme_register() -> Result<(), Errno> {
    let name = b"attach";
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

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register().is_err() {
        return 1;
    }
    let mut bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match recv(&mut bytes) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut reply = match raven_ipc::protocol::decode(&bytes) {
            Ok(packet) => packet,
            Err(_) => return 2,
        };
        if reply.opcode != Opcode::Open.as_u16() {
            reply.status = i32::from(Errno::EPROTO.0);
        } else {
            reply.status = 0;
            reply.flags |= REPLY_HANDLE;
            reply.attach_count = 1;
            reply.attach[0] = Attachment {
                handle: TRANSFERRED.0,
                rights_mask: Rights::READ.union(Rights::MAP).0,
            };
        }
        let mut response = [0u8; PACKET_SIZE];
        if encode(&reply, &mut response).is_err() || send(request, &response).is_err() {
            return 3;
        }
    }
}
