// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S39 fixture scheme server: registers the granted and denied service names and answers the
//! control handshake used by the namespace/event test.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_s39_parity::{DENIED_SCHEME, GRANTED_SCHEME};
use raven_scheme::{CallReply, Ctx, Handler, HandlerResult, Opened, Server};

entry!(main);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Data,
    Control,
}

struct OpenFile {
    kind: Kind,
    cursor: usize,
}

struct HandlerImpl {
    files: BTreeMap<u64, OpenFile>,
    next_file: u64,
    data: Vec<u8>,
    armed: bool,
}

impl HandlerImpl {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            next_file: 1,
            data: Vec::new(),
            armed: false,
        }
    }

    fn reply(response: &mut [u8], bytes: &[u8]) -> usize {
        let length = bytes.len().min(response.len());
        if let (Some(destination), Some(source)) = (response.get_mut(..length), bytes.get(..length))
        {
            destination.copy_from_slice(source);
        }
        length
    }
}

impl Handler for HandlerImpl {
    fn open(&mut self, path: &str, _flags: u32, _mode: u32, _ctx: &Ctx) -> HandlerResult<Opened> {
        let name = path.trim_start_matches('/');
        let kind = match name {
            "" | "file" => Kind::Data,
            "ctl" => Kind::Control,
            _ => return Err(Errno::ENOENT.into()),
        };
        let id = self.next_file;
        self.next_file = self.next_file.saturating_add(1);
        self.files.insert(id, OpenFile { kind, cursor: 0 });
        Ok(Opened::File(id))
    }

    fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        let Some(open) = self.files.get_mut(&file) else {
            return Err(Errno::EBADF.into());
        };
        if open.kind != Kind::Data {
            return Ok(0);
        }
        let start = open.cursor.min(self.data.len());
        let available = self.data.len().saturating_sub(start);
        let length = available.min(buffer.len());
        if let (Some(source), Some(destination)) = (
            self.data.get(start..start.saturating_add(length)),
            buffer.get_mut(..length),
        ) {
            destination.copy_from_slice(source);
        }
        open.cursor = start.saturating_add(length);
        Ok(length)
    }

    fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        let Some(open) = self.files.get_mut(&file) else {
            return Err(Errno::EBADF.into());
        };
        if open.kind != Kind::Data {
            return Err(Errno::EACCES.into());
        }
        let start = open.cursor.min(self.data.len());
        if start.saturating_add(buffer.len()) > self.data.len() {
            self.data.resize(start.saturating_add(buffer.len()), 0);
        }
        if let (Some(destination), Some(source)) = (
            self.data.get_mut(start..start.saturating_add(buffer.len())),
            Some(buffer),
        ) {
            destination.copy_from_slice(source);
        }
        open.cursor = start.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.files.remove(&file);
        Ok(())
    }

    fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        let Some(open) = self.files.get(&file) else {
            return Err(Errno::EBADF.into());
        };
        match open.kind {
            Kind::Control if request == b"armed" => {
                self.armed = true;
                Ok(Self::reply(response, b"ok"))
            }
            Kind::Control if request == b"state" => {
                let state: &[u8] = if self.armed { b"armed" } else { b"wait" };
                Ok(Self::reply(response, state))
            }
            Kind::Control if request == b"ping" => Ok(Self::reply(response, b"pong")),
            Kind::Control => Err(Errno::EINVAL.into()),
            Kind::Data => Ok(Self::reply(response, request)),
        }
    }

    fn call_attach(
        &mut self,
        file: u64,
        request: &[u8],
        response: &mut [u8],
    ) -> HandlerResult<CallReply> {
        Ok(CallReply::new(self.call(file, request, response)?))
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let mut server = match Server::register(GRANTED_SCHEME) {
        Ok(server) => server,
        Err(error) => {
            println!("s39: register failed: {}", error.description());
            return 1;
        }
    };
    // The second name is intentionally outside the client's granted prefix; the kernel must
    // reject its OPEN before any request reaches this server.
    let _denied = match Server::register(DENIED_SCHEME) {
        Ok(server) => server,
        Err(error) => {
            println!("s39: denied register failed: {}", error.description());
            return 2;
        }
    };
    let mut handler = HandlerImpl::new();
    println!("s39: server registered");
    loop {
        if let Some(packet) = server.poll(None) {
            let _ = server.dispatch(packet, &mut handler);
        }
    }
}
