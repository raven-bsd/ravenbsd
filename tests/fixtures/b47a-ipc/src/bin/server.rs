// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B47a fixture scheme server: data file, control file and a deferred client-death request.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::{
    CallReply, Completer, Ctx, Handler, HandlerError, HandlerResult, Opened, Server,
};

entry!(main);

const HOLD_TOKEN: u64 = 0x1000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Data,
    Control,
    Hold,
}

struct OpenFile {
    kind: Kind,
    cursor: usize,
}

struct HandlerImpl {
    completer: Completer,
    files: BTreeMap<u64, OpenFile>,
    next_file: u64,
    data: Vec<u8>,
    held: Option<u64>,
}

impl HandlerImpl {
    fn new(completer: Completer) -> Self {
        Self {
            completer,
            files: BTreeMap::new(),
            next_file: 1,
            data: Vec::new(),
            held: None,
        }
    }
}

impl Handler for HandlerImpl {
    fn open(&mut self, path: &str, _flags: u32, _mode: u32, _ctx: &Ctx) -> HandlerResult<Opened> {
        let name = path.trim_start_matches('/');
        let kind = match name {
            "" | "file" => Kind::Data,
            "ctl" => Kind::Control,
            "hold" => Kind::Hold,
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
            Kind::Hold => {
                let token = HOLD_TOKEN.saturating_add(self.next_file);
                self.held = Some(token);
                println!("b47a: held");
                Err(HandlerError::Defer(token))
            }
            Kind::Control if request == b"state" => {
                let reply: &[u8] = if self.held.is_some() {
                    b"held"
                } else {
                    b"wait"
                };
                let length = reply.len().min(response.len());
                if let (Some(destination), Some(source)) =
                    (response.get_mut(..length), reply.get(..length))
                {
                    destination.copy_from_slice(source);
                }
                Ok(length)
            }
            Kind::Control if request == b"release" => {
                let release = match self.held.take() {
                    Some(token) => match self.completer.complete(token, Err(Errno::EIO)) {
                        Ok(()) => b"sent".as_slice(),
                        Err(Errno::EIO) => {
                            println!("b47a: client death EIO");
                            b"released".as_slice()
                        }
                        Err(_) => b"failed".as_slice(),
                    },
                    None => b"none".as_slice(),
                };
                let length = release.len().min(response.len());
                if let (Some(destination), Some(source)) =
                    (response.get_mut(..length), release.get(..length))
                {
                    destination.copy_from_slice(source);
                }
                Ok(length)
            }
            Kind::Control => Err(Errno::EINVAL.into()),
            Kind::Data => {
                let length = request.len().min(response.len());
                if let (Some(destination), Some(source)) =
                    (response.get_mut(..length), request.get(..length))
                {
                    destination.copy_from_slice(source);
                }
                Ok(length)
            }
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
    let mut server = match Server::register("b47a") {
        Ok(server) => server,
        Err(error) => {
            println!("b47a: register failed: {}", error.description());
            return 1;
        }
    };
    let completer = server.completer();
    let mut handler = HandlerImpl::new(completer);
    println!("b47a: server registered");
    loop {
        if let Some(packet) = server.poll(None) {
            let _ = server.dispatch(packet, &mut handler);
        }
    }
}
