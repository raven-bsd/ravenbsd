// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD read-only filesystem server over the initfs (`initfs:` scheme).

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_initfs::Initfs;
    use raven_initfsd::model::{FileEntry, Model, NodeRef};
    use raven_proto_fs::{
        Call, CallRequest, CallResponse, OpenFlags, S_IFDIR, S_IFMT, Stat, StatFs,
    };
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, OpenTable, Opened, Server};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    /// Aggregate open-file-description budget.
    const MAX_OPEN_FILES: usize = 256;

    struct OpenState {
        node: NodeRef,
    }

    struct InitfsHandler {
        model: Model<'static>,
        opens: OpenTable<OpenState>,
        pending: Option<(u64, String)>,
    }

    impl Handler for InitfsHandler {
        fn open_with(
            &mut self,
            path: &str,
            flags: OpenFlags,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path {
                return Err(Errno::EINVAL.into());
            }
            if flags.writable() || flags.create() {
                return Err(Errno::EROFS.into());
            }
            if !flags.readable() {
                return Err(Errno::EBADF.into());
            }
            if self.opens.len() >= MAX_OPEN_FILES {
                return Err(Errno::EMFILE.into());
            }
            let node = self.model.lookup(path)?;
            if flags.directory() && self.model.stat(node)?.mode & S_IFMT != S_IFDIR {
                return Err(Errno::ENOTDIR.into());
            }
            self.opens
                .insert_with_id(file_id, OpenState { node }, flags)?;
            Ok(Opened::File(file_id))
        }

        fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if offset != 0 {
                return Err(Errno::EINVAL.into());
            }
            let Some(open) = self.opens.get_mut(file) else {
                return Err(Errno::EBADF.into());
            };
            let cursor = open.cursor();
            let read = match open.file.node {
                NodeRef::File(index) => self.model.read_file(index, cursor, buffer)?,
                NodeRef::Dir(index) => self.model.readdir(index, cursor, buffer)?,
            };
            open.advance(read)?;
            Ok(read)
        }

        fn write(&mut self, _file: u64, _offset: u64, _buffer: &[u8]) -> HandlerResult<usize> {
            Err(Errno::EROFS.into())
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.opens.close(file)?;
            Ok(())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            Ok(self.opens.dup(file)?)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let node = self.opens.get(file).ok_or(Errno::EBADF)?.file.node;
            Ok(self.model.stat(node)?)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            if self.opens.get(file).is_none() {
                return Err(Errno::EBADF.into());
            }
            let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
            match decoded.call {
                Call::Statfs => {
                    let statfs: StatFs = self.model.statfs();
                    CallResponse::StatFs { status: 0, statfs }
                        .encode(response)
                        .map_err(|_| Errno::EPROTO.into())
                }
                _ => status(response, Errno::EROFS),
            }
        }
    }

    fn status(response: &mut [u8], error: Errno) -> HandlerResult<usize> {
        CallResponse::Status(-(error.0 as i32))
            .encode(response)
            .map_err(|_| Errno::EPROTO.into())
    }

    fn main(args: raven_rt::Args) -> i32 {
        if let Some(value) = args.env_get("INITFS_HANDLE")
            && value != "1"
        {
            raven_rt::eprintln!("initfsd: INITFS_HANDLE must be 1, got {value}");
            return 1;
        }
        let image = match raven_rt::map_initfs() {
            Ok(image) => image,
            Err(error) => {
                raven_rt::eprintln!("initfsd: initfs map: {}", error.description());
                return 1;
            }
        };
        let initfs = match Initfs::parse(image) {
            Ok(initfs) => initfs,
            Err(_) => {
                raven_rt::eprintln!("initfsd: invalid initfs");
                return 1;
            }
        };
        let entries: Vec<FileEntry<'static>> = initfs
            .entries()
            .map(|entry| FileEntry {
                path: entry.path,
                mode: entry.mode,
                data: entry.data,
            })
            .collect();
        let model = match Model::new(&entries) {
            Ok(model) => model,
            Err(_) => {
                raven_rt::eprintln!("initfsd: cannot build the path tree");
                return 1;
            }
        };
        let mut server = match Server::register("initfs") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("initfsd: {}", error.description());
                return 1;
            }
        };
        log_started("initfsd");
        let mut handler = InitfsHandler {
            model,
            opens: OpenTable::new(),
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

    fn log_started(name: &str) {
        let client = raven_scheme::Client::new();
        let path = alloc::format!("log:{name}");
        if let Ok(handle) = client.open(&path, 2) {
            let _ = client.write(handle, 0, b"started\n");
            let _ = client.close(handle);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
