// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Q3 trace-context scheme server: registers `trace` and returns one byte for READ, so the
//! controller can prove the call/reply path is green.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

fn send_reply(request: raven_abi::types::Handle, reply: &SchemePacket) -> Result<(), Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(reply, &mut bytes).map_err(|_| Errno::EPROTO)?;
    raven_rt::sys::send(request, &bytes)
}

fn main(_args: raven_rt::Args) -> i32 {
    if raven_rt::sys::scheme_register("trace").is_err() {
        println!("trace: server register failed");
        return 1;
    }
    println!("trace: server ready");

    let mut packet_bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match raven_rt::sys::recv(&mut packet_bytes) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut reply = match protocol::decode(&packet_bytes) {
            Ok(reply) => reply,
            Err(_) => continue,
        };
        reply.status = 0;
        match reply.opcode {
            value if value == Opcode::Open.as_u16() => {
                reply.server_cookie = 1;
                reply.buffer_len = 0;
            }
            value if value == Opcode::Read.as_u16() => {
                let length = usize::try_from(reply.buffer_len).unwrap_or(0).min(1);
                if length > 0 {
                    // SAFETY: the kernel captured `buffer_len` writable bytes at `buffer_addr`
                    // in this address space for the duration of the request.
                    unsafe {
                        (reply.buffer_addr as *mut u8).write(b'R');
                    }
                }
                reply.buffer_len = length as u64;
            }
            value if value == Opcode::Close.as_u16() || value == Opcode::Dup.as_u16() => {}
            _ => {
                reply.status = i32::from(Errno::EPROTO.0);
            }
        }
        let _ = send_reply(request, &reply);
    }
}
