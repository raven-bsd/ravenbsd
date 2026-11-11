// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S33 scheme server. It serves the normal echo paths and, on command, exercises the three
//! lifecycle defects under test: an invalid `RECV` destination, one-way accounting and reply
//! bounds that exceed the request's buffers.

#![no_std]
#![no_main]

extern crate alloc;

use raven_abi::errno::Errno;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_rt::entry;
use raven_rt::println;
use raven_s33_ipc::{
    decode_packet, encode_packet, exit, nanosleep, recv, recv_raw, scheme_register, send,
};

entry!(main);

/// Server-side file mode, returned to clients as `server_cookie`.
const MODE_NORMAL: u64 = 1;
const MODE_BAD_WRITE: u64 = 2;
const MODE_BAD_CALL: u64 = 3;
const MODE_BAD_RECV: u64 = 4;
const MODE_LOOP: u64 = 5;
const MODE_BLOCK: u64 = 6;
const MODE_PAUSE: u64 = 7;
const MODE_ZERO_WRITE: u64 = 8;

/// Deliberately non-canonical destination for the K2 scenario.
const BAD_ADDRESS: usize = 0xFFFF_FFFF_FFFF_F000;
/// Longest payload the server stores.
const PAYLOAD: usize = 64 * 1024;

fn mode_for_path(path: &[u8]) -> u64 {
    match path {
        b"normal" => MODE_NORMAL,
        b"bad-write" => MODE_BAD_WRITE,
        b"bad-call" => MODE_BAD_CALL,
        b"bad-recv" => MODE_BAD_RECV,
        b"loop" => MODE_LOOP,
        b"block" => MODE_BLOCK,
        b"pause" => MODE_PAUSE,
        b"zero-write" => MODE_ZERO_WRITE,
        _ => MODE_NORMAL,
    }
}

enum CallKind {
    Block,
    Pause,
    ArmBadRecv,
    Exact,
    Bad,
    Plain,
}

fn call_kind(packet: &SchemePacket) -> CallKind {
    let length = usize::try_from(packet.buffer_len)
        .unwrap_or(usize::MAX)
        .min(protocol::MAX_DATA);
    if length == 0 {
        return CallKind::Plain;
    }
    let source = packet.buffer_addr as *const u8;
    // SAFETY: the kernel captured `buffer_len` readable bytes at `buffer_addr` in this address
    // space for the duration of the request; `length` is bounded by `MAX_DATA`.
    let request = unsafe { core::slice::from_raw_parts(source, length) };
    match request {
        b"block" => CallKind::Block,
        b"pause" => CallKind::Pause,
        b"arm" => CallKind::ArmBadRecv,
        b"exact" => CallKind::Exact,
        b"bad" => CallKind::Bad,
        _ => CallKind::Plain,
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register("s33").is_err() {
        println!("s33: register failed");
        return 1;
    }
    println!("s33: server ready");

    let mut data = alloc::vec![0u8; PAYLOAD];
    let mut stored = 0usize;
    let mut packet_bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match recv(&mut packet_bytes) {
            Ok(handle) => handle,
            Err(_) => continue,
        };
        let mut reply = match decode_packet(&packet_bytes) {
            Ok(packet) => packet,
            Err(_) => continue,
        };
        reply.status = 0;
        let mode = reply.server_cookie;
        let mut die = false;
        let mut pause_after = false;
        let mut bad_recv_after = false;
        match reply.opcode {
            value if value == Opcode::Open.as_u16() => {
                reply.server_cookie = mode_for_path(reply.path_bytes());
            }
            value if value == Opcode::Write.as_u16() => {
                let length = usize::try_from(reply.buffer_len)
                    .unwrap_or(usize::MAX)
                    .min(data.len());
                match mode {
                    MODE_BAD_WRITE => {
                        // K4: report one byte more than the client supplied.
                        reply.buffer_len = reply.buffer_len.saturating_add(1);
                    }
                    MODE_ZERO_WRITE => {
                        reply.buffer_len = 0;
                    }
                    _ => {
                        if length > 0 {
                            let source = reply.buffer_addr as *const u8;
                            // SAFETY: the kernel captured `buffer_len` bytes at `buffer_addr` in
                            // this address space for the duration of the request.
                            unsafe {
                                core::ptr::copy_nonoverlapping(source, data.as_mut_ptr(), length);
                            }
                        }
                        stored = length;
                        reply.buffer_len = length as u64;
                    }
                }
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
            value if value == Opcode::Call.as_u16() => match call_kind(&reply) {
                CallKind::Block if mode == MODE_BLOCK => {
                    die = true;
                }
                CallKind::Pause if mode == MODE_PAUSE => {
                    reply.buffer_len = 0;
                    pause_after = true;
                }
                CallKind::ArmBadRecv if mode == MODE_BAD_RECV => {
                    reply.buffer_len = 0;
                    bad_recv_after = true;
                }
                CallKind::Exact => {
                    reply.buffer_len = reply.response_len;
                }
                CallKind::Bad if mode == MODE_BAD_CALL => {
                    // K4: report one byte more than the client's response capacity.
                    reply.buffer_len = reply.response_len.saturating_add(1);
                }
                _ => {
                    reply.buffer_len = 0;
                }
            },
            value if value == Opcode::Close.as_u16() || value == Opcode::Dup.as_u16() => {}
            value if value == Opcode::Fstat.as_u16() => {
                reply.data_len = 0;
            }
            _ => {
                reply.status = i32::from(Errno::EPROTO.0);
            }
        }
        if die {
            // Receive without replying, then die: the kernel must fail the blocked client with
            // EIO when this process exits.
            let _ = nanosleep(50_000_000);
            exit(0);
        }
        match encode_packet(&reply) {
            Ok(bytes) => {
                if send(request, &bytes).is_err() {
                    println!("s33: server send rejected");
                }
            }
            Err(_) => {
                println!("s33: server encode failed");
            }
        }
        if bad_recv_after {
            // K2: the kernel pops the next request and must fail it with EPROTO because this
            // destination is invalid. The error returned here is expected and ignored.
            match recv_raw(BAD_ADDRESS, PACKET_SIZE) {
                Ok(_) => println!("s33: bad recv unexpectedly delivered"),
                Err(_) => {}
            }
        }
        if pause_after {
            // Leave the next one-way request unprocessed while its client exits.
            let _ = nanosleep(200_000_000);
        }
    }
}
