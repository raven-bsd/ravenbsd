// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K71 server: answers CALL with one or two MemObject attachments.
//!
//! The objects are created before the server loop. The `norights` command
//! attaches a duplicate whose rights exclude TRANSFER; the kernel must reject
//! the reply with EPERM and complete the client.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_rt::entry;
use raven_scheme::{CallReply, Ctx, Handler, HandlerResult, Opened, Server};

use raven_k71_scheme::patterned_object;

entry!(main);

const ATTACH_RIGHTS: u32 = Rights::READ.union(Rights::MAP).0;

struct AttachHandler {
    first: Handle,
    second: Handle,
    no_transfer: Handle,
}

impl Handler for AttachHandler {
    fn open(&mut self, _path: &str, _flags: u32, _mode: u32, _ctx: &Ctx) -> HandlerResult<Opened> {
        Ok(Opened::File(1))
    }

    fn call(&mut self, _file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        if request != b"plain" || response.len() < 5 {
            return Err(Errno::EINVAL.into());
        }
        response[..5].copy_from_slice(b"plain");
        Ok(5)
    }

    fn call_attach(
        &mut self,
        file: u64,
        request: &[u8],
        response: &mut [u8],
    ) -> HandlerResult<CallReply> {
        match request {
            b"attach" => {
                let mut reply = CallReply::with_data(b"ok")?;
                reply.attach(self.first, ATTACH_RIGHTS)?;
                Ok(reply)
            }
            b"attach-two" => {
                let mut reply = CallReply::with_data(b"")?;
                reply.attach(self.first, ATTACH_RIGHTS)?;
                reply.attach(self.second, ATTACH_RIGHTS)?;
                Ok(reply)
            }
            b"norights" => {
                let mut reply = CallReply::with_data(b"")?;
                reply.attach(self.no_transfer, 0)?;
                Ok(reply)
            }
            _ => Ok(CallReply::new(self.call(file, request, response)?)),
        }
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let first = match patterned_object(0xa5) {
        Ok(object) => object,
        Err(_) => return 1,
    };
    let second = match patterned_object(0x5a) {
        Ok(object) => object,
        Err(_) => return 2,
    };
    let no_transfer = match raven_rt::handle_dup(second, Rights::READ.union(Rights::MAP)) {
        Ok(handle) => handle,
        Err(_) => return 3,
    };
    let mut server = match Server::register("k71") {
        Ok(server) => server,
        Err(_) => return 4,
    };
    let mut handler = AttachHandler {
        first,
        second,
        no_transfer,
    };
    server.run(&mut handler)
}
