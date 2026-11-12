// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `svc:` control scheme.
//!
//! `svc:` lists services as K60 directory entries. `svc:/<name>` returns one
//! `<name> <state> <handle> <restarts>` line and accepts `START`, `STOP` and `RESTART` through
//! `CALL`. The server uses K30's [`Server`] for registration, packet receive/validation and safe
//! capture-buffer dispatch; the open path is bound to the kernel's file id just before
//! `dispatch`, because the K30 `Handler::open` signature does not expose that id.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_proto_fs::{DT_REG, DirEntry, DirEntryHeader, Errno, S_IFDIR, S_IFREG, Stat};
use raven_proto_svc::{MAX_LINE, Op};
use raven_rt::sync::Mutex;
use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

use crate::boot::{self, Core};

/// S11 `Opcode::Open` wire value.
const OPEN_OPCODE: u16 = 1;
/// S11 `OPEN_READ` flag.
const OPEN_READ: u32 = 1;
/// S11 `OPEN_CREATE` flag.
const OPEN_CREATE: u32 = 4;

struct OpenFile {
    data: Vec<u8>,
    cursor: usize,
    index: Option<usize>,
}

/// The `svc:` server thread.
pub fn thread(_: usize) {
    serve();
}

fn serve() {
    let Some(mut server): Option<Server> = boot::take_server() else {
        return;
    };
    let Some(core) = boot::core_handle() else {
        return;
    };
    let mut handler = SvcHandler {
        core,
        files: BTreeMap::new(),
        pending: None,
    };
    loop {
        let Some(packet) = server.poll(None) else {
            continue;
        };
        if packet.packet.opcode == OPEN_OPCODE
            && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
        {
            handler.pending = Some((packet.packet.file_id, path.to_string()));
        }
        let _ = server.dispatch(packet, &mut handler);
    }
}

struct SvcHandler {
    core: Arc<Mutex<Core>>,
    files: BTreeMap<u64, OpenFile>,
    pending: Option<(u64, String)>,
}

impl SvcHandler {
    fn directory(&self) -> HandlerResult<Vec<u8>> {
        let guard = self.core.lock();
        let mut data = Vec::new();
        for (index, name) in guard.supervisor.names().enumerate() {
            let entry = DirEntry {
                header: DirEntryHeader {
                    ino: index as u64 + 1,
                    kind: DT_REG,
                    name_len: name.len() as u8,
                    pad: [0; 6],
                },
                name: name.as_bytes(),
            };
            let length = entry.encoded_len();
            let start = data.len();
            data.resize(start + length, 0);
            let slot = data.get_mut(start..start + length).ok_or(Errno::ENOMEM)?;
            entry.encode(slot).map_err(|_| Errno::EPROTO)?;
        }
        Ok(data)
    }

    fn service(&self, name: &str) -> HandlerResult<OpenFile> {
        let guard = self.core.lock();
        let index = guard.supervisor.find(name).ok_or(Errno::ENOENT)?;
        let status = guard.supervisor.status(index).ok_or(Errno::ENOENT)?;
        let mut line = [0u8; MAX_LINE];
        let length = status.encode(&mut line).map_err(|_| Errno::EPROTO)?;
        Ok(OpenFile {
            data: line.get(..length).ok_or(Errno::EPROTO)?.to_vec(),
            cursor: 0,
            index: Some(index),
        })
    }
}

impl Handler for SvcHandler {
    fn open(&mut self, path: &str, flags: u32, _mode: u32, _ctx: &Ctx) -> HandlerResult<Opened> {
        let Some((file_id, expected)) = self.pending.take() else {
            return Err(Errno::EINVAL.into());
        };
        if expected != path {
            return Err(Errno::EINVAL.into());
        }
        if flags & OPEN_CREATE != 0 || flags & OPEN_READ == 0 {
            return Err(Errno::EACCES.into());
        }
        let file = if path.is_empty() || path == "/" {
            OpenFile {
                data: self.directory()?,
                cursor: 0,
                index: None,
            }
        } else {
            let name = path.strip_prefix('/').ok_or(Errno::EINVAL)?;
            if name.is_empty() || name.contains('/') {
                return Err(Errno::ENOENT.into());
            }
            self.service(name)?
        };
        self.files.insert(file_id, file);
        Ok(Opened::File(file_id))
    }

    fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        let Some(open) = self.files.get_mut(&file) else {
            return Err(Errno::EBADF.into());
        };
        let remaining = open.data.get(open.cursor..).unwrap_or(&[]);
        let length = remaining.len().min(buffer.len());
        let target = buffer.get_mut(..length).ok_or(Errno::EINVAL)?;
        target.copy_from_slice(remaining.get(..length).unwrap_or(&[]));
        open.cursor += length;
        Ok(length)
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.files
            .remove(&file)
            .map(|_| ())
            .ok_or_else(|| Errno::EBADF.into())
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        let Some(open) = self.files.get(&file) else {
            return Err(Errno::EBADF.into());
        };
        let size = open.data.len() as u64;
        let mode = if open.index.is_some() {
            S_IFREG | 0o444
        } else {
            S_IFDIR | 0o555
        };
        Ok(Stat {
            ino: file.wrapping_add(1),
            size,
            blocks: size.div_ceil(512),
            mode,
            nlink: 1,
            ..Stat::default()
        })
    }

    fn call(&mut self, file: u64, request: &[u8], _response: &mut [u8]) -> HandlerResult<usize> {
        let Some(open) = self.files.get(&file) else {
            return Err(Errno::EBADF.into());
        };
        let Some(index) = open.index else {
            return Err(Errno::EISDIR.into());
        };
        let operation = Op::decode(request).map_err(|_| Errno::EINVAL)?;
        let now = boot::now_ms();
        let mut guard = self.core.lock();
        match operation {
            Op::Start => {
                let Some(start) = guard.supervisor.request_start(index, now) else {
                    return Err(Errno::EBUSY.into());
                };
                guard.pending_starts.push(start);
            }
            Op::Stop => {
                if let Some(handle) = guard.supervisor.stop(index, false) {
                    guard.pending_terminations.push(handle);
                }
            }
            Op::Restart => {
                if let Some(handle) = guard.supervisor.stop(index, true) {
                    guard.pending_terminations.push(handle);
                } else {
                    let state = guard
                        .supervisor
                        .runtime()
                        .get(index)
                        .map(|runtime| runtime.state);
                    if matches!(
                        state,
                        Some(
                            raven_proto_svc::State::Stopped
                                | raven_proto_svc::State::Failed
                                | raven_proto_svc::State::GaveUp
                        )
                    ) {
                        let Some(start) = guard.supervisor.request_start(index, now) else {
                            return Err(Errno::EBUSY.into());
                        };
                        guard.pending_starts.push(start);
                    }
                }
            }
        }
        Ok(0)
    }
}
