// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S32 cooperative capture server. It echoes 1 MiB buffers, can delay a read/write while the
//! client unmaps its range, and can die with a capture installed.

#![no_std]
#![no_main]

extern crate alloc;

use raven_abi::errno::Errno;
use raven_ipc::protocol::{Opcode, PACKET_SIZE};
use raven_rt::entry;
use raven_rt::println;
use raven_s32_capture::{
    decode_packet, encode_packet, exit, fill, nanosleep, pattern_matches, recv, scheme_register,
    send,
};

entry!(main);

const MODE_ECHO: u64 = 1;
const MODE_DELAY_WRITE: u64 = 2;
const MODE_DELAY_READ: u64 = 3;
const MODE_DIE: u64 = 4;
const MODE_PAUSE: u64 = 5;

/// Size of the echo buffer and every fixture capture.
const LEN: usize = 1024 * 1024;

fn mode_for_path(path: &[u8]) -> u64 {
    match path {
        b"echo" => MODE_ECHO,
        b"delay-write" => MODE_DELAY_WRITE,
        b"delay-read" => MODE_DELAY_READ,
        b"die" => MODE_DIE,
        b"pause" => MODE_PAUSE,
        _ => MODE_ECHO,
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register("s32").is_err() {
        println!("s32: register failed");
        return 1;
    }
    println!("s32: server ready");

    let mut data = alloc::vec![0u8; LEN];
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
        match reply.opcode {
            value if value == Opcode::Open.as_u16() => {
                reply.server_cookie = mode_for_path(reply.path_bytes());
            }
            value if value == Opcode::Write.as_u16() => {
                let length = usize::try_from(reply.buffer_len)
                    .unwrap_or(usize::MAX)
                    .min(data.len());
                if mode == MODE_DELAY_WRITE {
                    // The client unmaps its range while this sleep runs; the kernel snapshot must
                    // still match the bytes the client wrote before the syscall.
                    let _ = nanosleep(150_000_000);
                    let source = reply.buffer_addr as *const u8;
                    // SAFETY: the kernel captured `buffer_len` readable bytes at `buffer_addr`
                    // in this address space for the duration of the request.
                    let ok = unsafe { pattern_matches(source as usize, length, 0x3c) };
                    if ok {
                        reply.buffer_len = length as u64;
                    } else {
                        println!("s32: server saw a wrong write snapshot");
                        reply.status = i32::from(Errno::EPROTO.0);
                    }
                } else {
                    if length > 0 {
                        let source = reply.buffer_addr as *const u8;
                        // SAFETY: the kernel captured `buffer_len` readable bytes at
                        // `buffer_addr` for the duration of the request.
                        unsafe {
                            core::ptr::copy_nonoverlapping(source, data.as_mut_ptr(), length);
                        }
                    }
                    stored = length;
                    reply.buffer_len = length as u64;
                }
            }
            value if value == Opcode::Read.as_u16() => {
                let length = usize::try_from(reply.buffer_len)
                    .unwrap_or(usize::MAX)
                    .min(data.len());
                match mode {
                    MODE_DIE => {
                        // Receive without replying, then die; the kernel must wake the client.
                        die = true;
                    }
                    MODE_DELAY_READ => {
                        let _ = nanosleep(150_000_000);
                        // The client unmaps its range during the sleep. The kernel maps these
                        // frames only in this process; the copy-back must skip the dead range.
                        // SAFETY: the kernel captured `buffer_len` writable bytes at
                        // `buffer_addr` in this address space.
                        unsafe {
                            fill(reply.buffer_addr as usize, length, 0x55);
                        }
                        reply.buffer_len = length as u64;
                    }
                    _ => {
                        let length = length.min(stored);
                        if length > 0 {
                            let destination = reply.buffer_addr as *mut u8;
                            // SAFETY: the kernel captured `buffer_len` writable bytes at
                            // `buffer_addr` in this address space.
                            unsafe {
                                core::ptr::copy_nonoverlapping(data.as_ptr(), destination, length);
                            }
                        }
                        reply.buffer_len = length as u64;
                    }
                }
            }
            value if value == Opcode::Call.as_u16() => {
                if mode == MODE_PAUSE {
                    let length = usize::try_from(reply.buffer_len)
                        .unwrap_or(usize::MAX)
                        .min(data.len());
                    let source = reply.buffer_addr as *const u8;
                    // SAFETY: the kernel captured `buffer_len` readable bytes at `buffer_addr`.
                    let request = if length > 0 {
                        unsafe { core::slice::from_raw_parts(source, length) }
                    } else {
                        &[]
                    };
                    if request == b"pause" {
                        pause_after = true;
                    }
                }
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
        if die {
            let _ = nanosleep(100_000_000);
            exit(0);
        }
        match encode_packet(&reply) {
            Ok(bytes) => {
                let _ = send(request, &bytes);
            }
            Err(_) => {
                println!("s32: server encode failed");
            }
        }
        if pause_after {
            // Leave the canceller's one-way CLOSE queued while its process exits.
            let _ = nanosleep(200_000_000);
        }
    }
}
