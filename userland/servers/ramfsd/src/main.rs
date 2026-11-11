// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `ramfs:` in-memory writable filesystem server.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};

    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_proto_fs::{Call, CallRequest, CallResponse, S_IFDIR, S_IFMT, Stat, StatFs};
    use raven_ramfs_core::{ROOT_INO, Ramfs};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    /// S11 `OPEN_READ` flag.
    const OPEN_READ: u32 = 1;
    /// S11 `OPEN_WRITE` flag.
    const OPEN_WRITE: u32 = 2;
    /// S11 `OPEN_CREATE` flag.
    const OPEN_CREATE: u32 = 4;

    struct OpenFile {
        ino: u64,
        readable: bool,
        writable: bool,
        cursor: u64,
    }

    struct RamfsHandler {
        fs: Ramfs,
        files: BTreeMap<u64, OpenFile>,
        refs: BTreeMap<u64, u32>,
        pending: Option<(u64, String)>,
    }

    impl RamfsHandler {
        fn new() -> Self {
            Self {
                fs: Ramfs::new(now_seconds()),
                files: BTreeMap::new(),
                refs: BTreeMap::new(),
                pending: None,
            }
        }

        fn reference(&mut self, ino: u64) {
            *self.refs.entry(ino).or_insert(0) += 1;
        }

        fn dereference(&mut self, ino: u64) {
            let Some(count) = self.refs.get_mut(&ino) else {
                return;
            };
            if *count > 1 {
                *count -= 1;
                return;
            }
            self.refs.remove(&ino);
            if self.fs.is_unlinked(ino) {
                let _ = self.fs.release(ino);
            }
        }

        fn release_if_unreferenced(&mut self, ino: u64) {
            if self.refs.get(&ino).copied().unwrap_or(0) == 0 {
                let _ = self.fs.release(ino);
            }
        }

        fn status(response: &mut [u8], result: Result<(), Errno>) -> HandlerResult<usize> {
            let status = match result {
                Ok(()) => 0,
                Err(error) => -(error.0 as i32),
            };
            CallResponse::Status(status)
                .encode(response)
                .map_err(|_| Errno::EPROTO.into())
        }

        fn status_with_ino(
            response: &mut [u8],
            result: Result<u64, Errno>,
            handler: &mut RamfsHandler,
        ) -> HandlerResult<usize> {
            match result {
                Ok(ino) => {
                    handler.release_if_unreferenced(ino);
                    Self::status(response, Ok(()))
                }
                Err(error) => Self::status(response, Err(error)),
            }
        }
    }

    impl Handler for RamfsHandler {
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
            let readable = flags & OPEN_READ != 0;
            let writable = flags & OPEN_WRITE != 0;
            let create = flags & OPEN_CREATE != 0;
            if !readable && !writable {
                return Err(Errno::EACCES.into());
            }
            let now = now_seconds();
            let ino = match self.fs.lookup(path) {
                Ok(ino) => ino,
                Err(Errno::ENOENT) if create => {
                    self.fs.create_file_at(ROOT_INO, path, 0o644, now)?
                }
                Err(error) => return Err(error.into()),
            };
            let stat = self.fs.stat(ino)?;
            let is_dir = stat.mode & S_IFMT == S_IFDIR;
            if is_dir && writable {
                return Err(Errno::EISDIR.into());
            }
            self.reference(ino);
            self.files.insert(
                file_id,
                OpenFile {
                    ino,
                    readable,
                    writable,
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
            let ino = open.ino;
            let cursor = open.cursor;
            let stat = self.fs.stat(ino)?;
            let read = if stat.mode & S_IFMT == S_IFDIR {
                self.fs.readdir(ino, cursor, buffer, now_seconds())?
            } else {
                self.fs.read(ino, cursor, buffer, now_seconds())?
            };
            open.cursor = cursor.saturating_add(read as u64);
            Ok(read)
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let Some(open) = self.files.get_mut(&file) else {
                return Err(Errno::EBADF.into());
            };
            if !open.writable {
                return Err(Errno::EBADF.into());
            }
            let ino = open.ino;
            let stat = self.fs.stat(ino)?;
            if stat.mode & S_IFMT == S_IFDIR {
                return Err(Errno::EISDIR.into());
            }
            let written = self.fs.write(ino, open.cursor, buffer, now_seconds())?;
            open.cursor = open.cursor.saturating_add(written as u64);
            Ok(written)
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            let open = self.files.remove(&file).ok_or(Errno::EBADF)?;
            self.dereference(open.ino);
            Ok(())
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let open = self.files.get(&file).ok_or(Errno::EBADF)?;
            Ok(self.fs.stat(open.ino)?)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let decoded = CallRequest::decode(request).map_err(|_| Errno::EPROTO)?;
            let dir_ino = self.files.get(&file).ok_or(Errno::EBADF)?.ino;
            let now = now_seconds();
            match decoded.call {
                Call::Mkdir { mode, path } => {
                    let path = text(path)?;
                    Self::status(
                        response,
                        self.fs
                            .mkdir_at(dir_ino, path, mode as u16, now)
                            .map(|_| ()),
                    )
                }
                Call::Unlink { path } => {
                    let path = text(path)?;
                    let result = self.fs.unlink_at(dir_ino, path, now);
                    Self::status_with_ino(response, result, self)
                }
                Call::Rmdir { path } => {
                    let path = text(path)?;
                    let result = self.fs.rmdir_at(dir_ino, path, now);
                    Self::status_with_ino(response, result, self)
                }
                Call::Rename { from, to } => {
                    let from = text(from)?;
                    let to = text(to)?;
                    match self.fs.rename_at(dir_ino, from, to, now) {
                        Ok(replaced) => {
                            if let Some(ino) = replaced {
                                self.release_if_unreferenced(ino);
                            }
                            Self::status(response, Ok(()))
                        }
                        Err(error) => Self::status(response, Err(error)),
                    }
                }
                Call::Truncate { size } => {
                    let open = self.files.get(&file).ok_or(Errno::EBADF)?;
                    let stat = self.fs.stat(open.ino)?;
                    if stat.mode & S_IFMT == S_IFDIR {
                        return Err(Errno::EISDIR.into());
                    }
                    let ino = open.ino;
                    Self::status(response, self.fs.truncate(ino, size, now))
                }
                Call::Symlink { path, target } => {
                    let path = text(path)?;
                    Self::status(
                        response,
                        self.fs.symlink_at(dir_ino, path, target, now).map(|_| ()),
                    )
                }
                Call::Readlink { path } => {
                    let path = text(path)?;
                    let target = self.fs.readlink_at(dir_ino, path)?;
                    CallResponse::Readlink {
                        status: 0,
                        target: &target,
                    }
                    .encode(response)
                    .map_err(|_| Errno::EPROTO.into())
                }
                Call::Chmod { mode, path } => {
                    let path = text(path)?;
                    Self::status(response, self.fs.chmod_at(dir_ino, path, mode as u16, now))
                }
                Call::Sync => Self::status(response, Ok(())),
                Call::Mount { .. } | Call::Umount { .. } => {
                    Self::status(response, Err(Errno::ENOSYS))
                }
                Call::Statfs => {
                    let statfs: StatFs = self.fs.statfs();
                    CallResponse::StatFs { status: 0, statfs }
                        .encode(response)
                        .map_err(|_| Errno::EPROTO.into())
                }
            }
        }
    }

    fn text(bytes: &[u8]) -> Result<&str, Errno> {
        core::str::from_utf8(bytes).map_err(|_| Errno::EINVAL)
    }

    fn now_seconds() -> i64 {
        let time = raven_rt::clock_get(clock::REALTIME)
            .or_else(|_| raven_rt::clock_get(clock::MONOTONIC))
            .unwrap_or_default();
        time.tv_sec.max(0)
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("ramfs") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("ramfsd: {}", error.description());
                return 1;
            }
        };
        log_started("ramfsd");
        let mut handler = RamfsHandler::new();
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
