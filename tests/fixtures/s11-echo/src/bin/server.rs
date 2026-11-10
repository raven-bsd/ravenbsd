// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S11 echo server: receives scheme packets and echoes the stored payload.

#![no_std]
#![no_main]

extern crate alloc;

use raven_abi::errno::Errno;
use raven_ipc::protocol::{Opcode, PACKET_SIZE, SchemePacket};
use raven_rt::entry;
use raven_rt::println;
use raven_s11_echo::{decode_packet, encode_packet, exit, recv, scheme_register, send};

entry!(main);

const CAPACITY: usize = 1024 * 1024;

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register("echo").is_err() {
        println!("scheme-echo: register failed");
        return 1;
    }
    println!("scheme-echo: server ready");

    let mut data = alloc::vec![0u8; CAPACITY];
    let mut stored = 0usize;
    let mut packet_bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match recv(&mut packet_bytes) {
            Ok(handle) => handle,
            Err(_) => continue,
        };
        let mut reply = match decode_packet(&packet_bytes) {
            Ok(packet) => packet,
            Err(_) => {
                let error = SchemePacket {
                    status: i32::from(Errno::EPROTO.0),
                    ..SchemePacket::default()
                };
                if let Ok(bytes) = encode_packet(&error) {
                    let _ = send(request, &bytes);
                }
                continue;
            }
        };
        reply.status = 0;
        match reply.opcode {
            value if value == Opcode::Open.as_u16() => {
                reply.server_cookie = reply.file_id;
            }
            value if value == Opcode::Write.as_u16() => {
                let length = usize::try_from(reply.buffer_len)
                    .unwrap_or(usize::MAX)
                    .min(data.len());
                if length > 0 {
                    let source = reply.buffer_addr as *const u8;
                    // SAFETY: the kernel captured `buffer_len` bytes at `buffer_addr` in this
                    // address space for the duration of the request.
                    unsafe {
                        core::ptr::copy_nonoverlapping(source, data.as_mut_ptr(), length);
                    }
                }
                stored = length;
                reply.buffer_len = length as u64;
            }
            value if value == Opcode::Read.as_u16() => {
                let length = usize::try_from(reply.buffer_len)
                    .unwrap_or(usize::MAX)
                    .min(stored);
                if length > 0 {
                    let destination = reply.buffer_addr as *mut u8;
                    // SAFETY: the kernel captured `buffer_len` writable bytes at `buffer_addr`.
                    unsafe {
                        core::ptr::copy_nonoverlapping(data.as_ptr(), destination, length);
                    }
                }
                reply.buffer_len = length as u64;
            }
            value if value == Opcode::Close.as_u16() => {}
            value if value == Opcode::Call.as_u16() => {
                let length = usize::try_from(reply.buffer_len)
                    .unwrap_or(usize::MAX)
                    .min(data.len());
                let request_bytes = if length > 0 {
                    // SAFETY: the kernel captured `buffer_len` readable bytes at `buffer_addr`.
                    unsafe { core::slice::from_raw_parts(reply.buffer_addr as *const u8, length) }
                } else {
                    &[]
                };
                if request_bytes == b"block" {
                    // Leave the controller's call pending and exit without a reply; the kernel
                    // must fail it with EIO when this server dies.
                    let _ = raven_s11_echo::nanosleep(50_000_000);
                    exit(0);
                }
                reply.data_len = 0;
            }
            value if value == Opcode::Fstat.as_u16() => {
                reply.data_len = 0;
            }
            _ => {
                reply.status = i32::from(Errno::EPROTO.0);
            }
        }
        match encode_packet(&reply) {
            Ok(bytes) => {
                let _ = send(request, &bytes);
            }
            Err(_) => {
                let _ = send(request, &packet_bytes);
            }
        }
    }
}
