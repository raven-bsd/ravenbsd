// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S40 test scheme server: serve OPEN/CLOSE normally, hold every `WRITE` on the hold path, and
//! release all held requests (EIO, because their clients are gone) when the `sync` control call
//! arrives.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{recv, scheme_register, send};

entry!(main);

/// File mode: reply normally.
const MODE_NORMAL: u64 = 0;
/// File mode: hold the WRITE reply until the sync control call.
const MODE_HOLD: u64 = 1;
/// File mode: FIFO barrier that releases the held requests.
const MODE_SYNC: u64 = 2;

fn mode_for_path(path: &[u8]) -> u64 {
    match path {
        b"hold" => MODE_HOLD,
        b"sync" => MODE_SYNC,
        _ => MODE_NORMAL,
    }
}

fn send_reply(request: Handle, reply: &SchemePacket) -> Result<(), Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(reply, &mut bytes).map_err(|_| Errno::EPROTO)?;
    send(request, &bytes)
}

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register("s40").is_err() {
        println!("s40: server register failed");
        return 1;
    }
    println!("s40: server ready");

    let mut held: Vec<(Handle, SchemePacket)> = Vec::new();
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
        let mode = reply.server_cookie;
        match reply.opcode {
            value if value == Opcode::Open.as_u16() => {
                reply.server_cookie = mode_for_path(reply.path_bytes());
                reply.buffer_len = 0;
            }
            value if value == Opcode::Write.as_u16() && mode == MODE_HOLD => {
                // The controller waits for each client to die before spawning the next one, so
                // every previously held request is cancelled. Release it before holding the new
                // one: the capture window has a fixed slot count and must stay bounded.
                for (held_request, held_reply) in held.drain(..) {
                    match send_reply(held_request, &held_reply) {
                        Ok(()) => println!("s40: hold send ok"),
                        Err(error) => println!("s40: hold send errno={}", error.0),
                    }
                }
                // Keep the new request handle; its writer stays blocked until the next hold or
                // the sync barrier, while the client faults in the meantime.
                held.push((request, reply));
                continue;
            }
            value if value == Opcode::Write.as_u16() => {}
            value if value == Opcode::Call.as_u16() && mode == MODE_SYNC => {
                // FIFO barrier: every earlier hold is in `held`. Release them all (their clients
                // are gone) and then answer the controller.
                for (held_request, held_reply) in held.drain(..) {
                    match send_reply(held_request, &held_reply) {
                        Ok(()) => println!("s40: hold send ok"),
                        Err(error) => println!("s40: hold send errno={}", error.0),
                    }
                }
                reply.buffer_len = 0;
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
