// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S37 death client: a one-shot scheme server that opens an unlinked file and returns it to the
//! controller as an `Open` `REPLY_HANDLE` attachment, then exits. The controller transfers the
//! attached reference to another process; the unlinked data must survive until that process
//! closes the last reference.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_ipc::protocol::{self, Attachment, Opcode, PACKET_SIZE, REPLY_HANDLE, SchemePacket};
use raven_proto_fs::OpenFlags;
use raven_rt::entry;
use raven_rt::println;
use raven_s37_fs::{open, recv, scheme_register, send};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    if scheme_register("death").is_err() {
        println!("s37: death client register failed");
        return 1;
    }
    let mut bytes = [0u8; PACKET_SIZE];
    let request = match recv(&mut bytes) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: death client recv failed");
            return 1;
        }
    };
    let mut reply = match protocol::decode(&bytes) {
        Ok(packet) => packet,
        Err(_) => {
            println!("s37: death client decode failed");
            return 1;
        }
    };
    if reply.opcode != Opcode::Open.as_u16() {
        println!("s37: death client expected open");
        return 1;
    }
    reply.status = 0;
    let file = match open("fs:unlinked", OpenFlags::READ.to_wire()) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: death client unlinked open failed");
            let error = SchemePacket {
                status: i32::from(Errno::EIO.0),
                ..SchemePacket::default()
            };
            if let Ok(bytes) = protocol::to_vec(&error) {
                let _ = send(request, &bytes);
            }
            return 1;
        }
    };
    reply.flags |= REPLY_HANDLE;
    reply.attach_count = 1;
    reply.attach[0] = Attachment {
        handle: file.0,
        rights_mask: Rights::READ.bits() | Rights::DUP.bits() | Rights::TRANSFER.bits(),
    };
    if protocol::encode(&reply, &mut bytes).is_err() || send(request, &bytes).is_err() {
        println!("s37: death client reply failed");
        return 1;
    }
    println!("s37: death client done");
    // Process exit drops this handle; the controller's attachment keeps the description alive.
    0
}
