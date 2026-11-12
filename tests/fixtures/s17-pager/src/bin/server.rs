// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S17 pager server: owns a 64 MiB synthetic file and serves `PageIn`/`PageOut` through the
//! scheme `CALL` opcode. `PAGER_SUPPLY` is the supply half of the contract (C18).

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, memobj, prot};
use raven_ipc::protocol::{self, Attachment, Opcode, PACKET_SIZE, REPLY_HANDLE, SchemePacket};
use raven_pager::{PAGE_IN, PAGE_OUT, PagerPacket};
use raven_rt::entry;
use raven_rt::println;
use raven_s17_pager::{
    PAGE, encode_reply, exit, map_object, memobj_create, page_pattern, pager_supply, recv,
    scheme_register, send, unmap,
};

entry!(main);

const MIB: usize = 1024 * 1024;
const NORMAL_BYTES: usize = 64 * MIB;
const STATS_LEN: usize = 6 * 8;
const STATS_COOKIE: u64 = 0x5354_4154_5331_3700;
const WRITE_PREFIX: &[u8] = b"s17-shared!";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Normal,
    Error,
    Death,
}

struct Objects {
    normal: Handle,
    error: Handle,
    death: Handle,
}

#[derive(Default)]
struct Stats {
    normal_page_in: u64,
    normal_page_out: u64,
    writeback_ok: u64,
    error_page_in: u64,
    error_short: u64,
    death_page_in: u64,
}

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register("pager").is_err() {
        return 1;
    }
    let normal = match memobj_create(NORMAL_BYTES, memobj::PAGER) {
        Ok(object) => object,
        Err(_) => return 2,
    };
    let error = match memobj_create(PAGE, memobj::PAGER) {
        Ok(object) => object,
        Err(_) => return 3,
    };
    let death = match memobj_create(PAGE, memobj::PAGER) {
        Ok(object) => object,
        Err(_) => return 4,
    };
    let objects = Objects {
        normal,
        error,
        death,
    };
    let mut stats = Stats::default();
    println!("s17: server ready");

    let mut bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match recv(&mut bytes) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut reply = match protocol::decode(&bytes) {
            Ok(packet) => packet,
            Err(_) => return 5,
        };
        reply.status = 0;
        if reply.opcode == Opcode::Open.as_u16() {
            handle_open(&mut reply, &objects);
        } else if reply.opcode == Opcode::Call.as_u16() {
            handle_call(&mut reply, &objects, &mut stats);
        } else if reply.opcode == Opcode::Close.as_u16() || reply.opcode == Opcode::Dup.as_u16() {
            // No server state to retire in this fixture.
        } else {
            reply.status = i32::from(Errno::EPROTO.0);
        }
        // The request's inline payload is not part of a Call reply (the response buffer is
        // written directly); leaving data_len set would trip the kernel's reply bound.
        reply.data_len = 0;
        let response = match encode_reply(&reply) {
            Ok(response) => response,
            Err(_) => return 6,
        };
        if send(request, &response).is_err() {
            return 7;
        }
    }
}

fn handle_open(reply: &mut SchemePacket, objects: &Objects) {
    match reply.path_bytes() {
        b"object" => attach(reply, objects.normal),
        b"error" => attach(reply, objects.error),
        b"death" => attach(reply, objects.death),
        b"stats" => reply.server_cookie = STATS_COOKIE,
        _ => reply.status = i32::from(Errno::ENOENT.0),
    }
}

fn attach(reply: &mut SchemePacket, object: Handle) {
    reply.flags |= REPLY_HANDLE;
    reply.attach_count = 1;
    reply.attach[0] = Attachment {
        handle: object.0,
        rights_mask: Rights::READ.union(Rights::WRITE).union(Rights::MAP).0,
    };
}

fn kind_of(objects: &Objects, key: u32) -> Option<Kind> {
    if key == objects.normal.0 {
        Some(Kind::Normal)
    } else if key == objects.error.0 {
        Some(Kind::Error)
    } else if key == objects.death.0 {
        Some(Kind::Death)
    } else {
        None
    }
}

fn handle_call(reply: &mut SchemePacket, objects: &Objects, stats: &mut Stats) {
    if let Ok(packet) = PagerPacket::decode(reply.data_bytes()) {
        match packet.kind {
            PAGE_IN => page_in(reply, objects, stats, &packet),
            PAGE_OUT => page_out(reply, objects, stats, &packet),
            _ => reply.status = i32::from(Errno::EPROTO.0),
        }
        return;
    }
    const STATS_COMMAND: &[u8] = b"stats";
    if reply.buffer_len == STATS_COMMAND.len() as u64 && reply.buffer_addr != 0 {
        // SAFETY: the kernel captured `buffer_len` readable bytes at `buffer_addr` in this
        // address space for the duration of the request.
        let command = unsafe {
            core::slice::from_raw_parts(reply.buffer_addr as *const u8, STATS_COMMAND.len())
        };
        if command == STATS_COMMAND {
            write_stats(reply, stats);
            return;
        }
    }
    reply.status = i32::from(Errno::EPROTO.0);
}

fn page_in(reply: &mut SchemePacket, objects: &Objects, stats: &mut Stats, packet: &PagerPacket) {
    match kind_of(objects, packet.object) {
        Some(Kind::Normal) => {
            let offset = match usize::try_from(packet.offset) {
                Ok(offset) => offset,
                Err(_) => {
                    reply.status = i32::from(Errno::EINVAL.0);
                    return;
                }
            };
            let mut buffer = [0u8; PAGE];
            page_pattern((offset / PAGE) as u64, &mut buffer);
            match pager_supply(objects.normal, offset, &buffer) {
                Ok(()) => stats.normal_page_in = stats.normal_page_in.saturating_add(1),
                Err(error) => reply.status = i32::from(error.0),
            }
        }
        Some(Kind::Error) => {
            stats.error_page_in = stats.error_page_in.saturating_add(1);
            let offset = match usize::try_from(packet.offset) {
                Ok(offset) => offset,
                Err(_) => {
                    reply.status = i32::from(Errno::EINVAL.0);
                    return;
                }
            };
            // C18: a zero-length supply is invalid and must not complete the page. The failed
            // fault is then woken by the error reply below.
            if pager_supply(objects.error, offset, &[]).is_err() {
                stats.error_short = stats.error_short.saturating_add(1);
            }
            reply.status = i32::from(Errno::EIO.0);
        }
        Some(Kind::Death) => {
            stats.death_page_in = stats.death_page_in.saturating_add(1);
            println!("s17: server pager death object={}", packet.object);
            // Exit without replying: the waiting fault must be failed with EIO and the
            // controller must observe the dead pager instead of a hang.
            exit(0);
        }
        None => reply.status = i32::from(Errno::EPROTO.0),
    }
}

fn page_out(reply: &mut SchemePacket, objects: &Objects, stats: &mut Stats, packet: &PagerPacket) {
    if kind_of(objects, packet.object) != Some(Kind::Normal) {
        reply.status = i32::from(Errno::EPROTO.0);
        return;
    }
    let offset = match usize::try_from(packet.offset) {
        Ok(offset) => offset,
        Err(_) => {
            reply.status = i32::from(Errno::EINVAL.0);
            return;
        }
    };
    let address = match map_object(objects.normal, offset, PAGE, prot::READ, true) {
        Ok(address) => address,
        Err(error) => {
            reply.status = i32::from(error.0);
            return;
        }
    };
    let mut prefix = [0u8; 16];
    let length = prefix.len().min(WRITE_PREFIX.len());
    // SAFETY: the mapping is one readable object page at `address`; the copy stays inside it.
    unsafe {
        core::ptr::copy_nonoverlapping(address as *const u8, prefix.as_mut_ptr(), length);
    }
    stats.normal_page_out = stats.normal_page_out.saturating_add(1);
    if prefix.get(..length) == Some(WRITE_PREFIX) {
        stats.writeback_ok = stats.writeback_ok.saturating_add(1);
    }
    if unmap(address, PAGE).is_err() {
        reply.status = i32::from(Errno::EIO.0);
    }
}

fn write_stats(reply: &mut SchemePacket, stats: &Stats) {
    if reply.response_len < STATS_LEN as u64 || reply.response_addr == 0 {
        reply.status = i32::from(Errno::EINVAL.0);
        return;
    }
    let words = [
        stats.normal_page_in,
        stats.normal_page_out,
        stats.writeback_ok,
        stats.error_page_in,
        stats.error_short,
        stats.death_page_in,
    ];
    let destination = reply.response_addr as *mut u8;
    let mut index = 0usize;
    for word in words {
        let bytes = word.to_le_bytes();
        // SAFETY: the kernel captured `response_len >= STATS_LEN` writable bytes at
        // `response_addr` for the duration of this request.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                destination.add(index.saturating_mul(8)),
                bytes.len(),
            );
        }
        index = index.saturating_add(1);
    }
    reply.buffer_len = STATS_LEN as u64;
}
