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
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_initfs::Initfs;
    use raven_initfsd::model::{FileEntry, Model, NodeRef};
    use raven_proto_fs::{Call, CallRequest, CallResponse, Stat, StatFs};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;
    const OPEN_CREATE: u32 = 4;

    struct OpenFile {
        node: NodeRef,
        readable: bool,
        cursor: u64,
    }

    struct InitfsHandler {
        model: Model<'static>,
        files: BTreeMap<u64, OpenFile>,
        pending: Option<(u64, String)>,
    }

    impl Handler for InitfsHandler {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path {
                return Err(Errno::EINVAL.into());
            }
            if flags & (OPEN_WRITE | OPEN_CREATE) != 0 {
                return Err(Errno::EROFS.into());
            }
            if flags & OPEN_READ == 0 {
                return Err(Errno::EACCES.into());
            }
            let node = self.model.lookup(path)?;
            self.files.insert(
                file_id,
                OpenFile {
                    node,
                    readable: true,
                    cursor: 0,
                },
            );
            Ok(Opened::File(file_id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let Some(open) = self.files.get_mut(&file) else {
                return Err(Errno::EBADF.into());
            };
            if !open.readable {
                return Err(Errno::EBADF.into());
            }
            let read = match open.node {
                NodeRef::File(index) => self.model.read_file(index, open.cursor, buffer)?,
                NodeRef::Dir(index) => self.model.readdir(index, open.cursor, buffer)?,
            };
            open.cursor = open.cursor.saturating_add(read as u64);
            Ok(read)
        }

        fn write(&mut self, _file: u64, _offset: u64, _buffer: &[u8]) -> HandlerResult<usize> {
            Err(Errno::EROFS.into())
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files.remove(&file).map(|_| ()).ok_or(Errno::EBADF)?;
            Ok(())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            if !self.files.contains_key(&file) {
                return Err(Errno::EBADF.into());
            }
            Ok(file)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let open = self.files.get(&file).ok_or(Errno::EBADF)?;
            Ok(self.model.stat(open.node)?)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            if !self.files.contains_key(&file) {
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
