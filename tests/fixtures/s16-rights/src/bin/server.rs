// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16 scheme server: registers `s16a`, `other` and `exact`, and returns one byte for READ.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_rt::entry;
use raven_rt::println;
use raven_s16_rights::{recv, scheme_register, send};

entry!(main);

fn send_reply(request: raven_abi::types::Handle, reply: &SchemePacket) -> Result<(), Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(reply, &mut bytes).map_err(|_| Errno::EPROTO)?;
    send(request, &bytes)
}

fn main(_args: raven_rt::Args) -> i32 {
    for name in ["s16a", "other", "exact"] {
        if scheme_register(name).is_err() {
            println!("s16: server register failed");
            return 1;
        }
    }
    println!("s16: server ready");

    let mut packet_bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match recv(&mut packet_bytes) {
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
            value if value == Opcode::Call.as_u16() => {
                reply.buffer_len = 0;
            }
            value if value == Opcode::Close.as_u16() || value == Opcode::Dup.as_u16() => {}
            value if value == Opcode::Fstat.as_u16() => {
                reply.data_len = 0;
            }
            _ => {
                reply.status = i32::from(Errno::EPROTO.0);
            }
        }
        let _ = send_reply(request, &reply);
    }
}
