// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `sys:` system information server.

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
    use raven_abi::types::clock;
    use raven_proto_fs::{DT_REG, DirEntry, DirEntryHeader, S_IFDIR, S_IFREG, Stat};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;
    const OPEN_CREATE: u32 = 4;

    #[derive(Clone, Copy)]
    enum Kind {
        Directory,
        Hostname,
        Uname,
        Version,
        Uptime,
        Kernel,
    }

    struct OpenFile {
        kind: Kind,
        cursor: usize,
    }

    struct SysHandler {
        hostname: Vec<u8>,
        directory: Vec<u8>,
        files: BTreeMap<u64, OpenFile>,
        pending: Option<(u64, String)>,
    }

    impl SysHandler {
        fn new() -> Self {
            let mut directory = Vec::new();
            for (index, name) in raven_sysd::FILES.iter().enumerate() {
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
                let start = directory.len();
                directory.resize(start + length, 0);
                if let Some(slot) = directory.get_mut(start..start + length) {
                    let _ = entry.encode(slot);
                }
            }
            Self {
                hostname: b"raven".to_vec(),
                directory,
                files: BTreeMap::new(),
                pending: None,
            }
        }

        fn kind_for(name: &str) -> Option<Kind> {
            match name {
                "hostname" => Some(Kind::Hostname),
                "uname" => Some(Kind::Uname),
                "version" => Some(Kind::Version),
                "uptime" => Some(Kind::Uptime),
                "kernel" => Some(Kind::Kernel),
                _ => None,
            }
        }

        fn content(&mut self, kind: Kind) -> Result<Vec<u8>, Errno> {
            match kind {
                Kind::Directory => Ok(self.directory.clone()),
                Kind::Hostname => Ok(raven_sysd::hostname_text(&self.hostname)),
                Kind::Uname => raven_rt::uname()
                    .map(|uname| raven_sysd::uname_text(&uname))
                    .map_err(|_| Errno::EIO),
                Kind::Version => raven_rt::uname()
                    .map(|uname| raven_sysd::version_text(&uname))
                    .map_err(|_| Errno::EIO),
                Kind::Kernel => raven_rt::uname()
                    .map(|uname| raven_sysd::kernel_text(&uname))
                    .map_err(|_| Errno::EIO),
                Kind::Uptime => match raven_rt::clock_get(clock::MONOTONIC) {
                    Ok(time) => Ok(raven_sysd::uptime_text(
                        u64::try_from(time.tv_sec).unwrap_or(0),
                    )),
                    Err(_) => Err(Errno::EIO),
                },
            }
        }
    }

    impl Handler for SysHandler {
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
            if expected != path || flags & OPEN_CREATE != 0 {
                return Err(Errno::EINVAL.into());
            }
            let readable = flags & OPEN_READ != 0;
            let writable = flags & OPEN_WRITE != 0;
            let kind = if path.is_empty() || path == "/" {
                if !readable || writable {
                    return Err(Errno::EACCES.into());
                }
                Kind::Directory
            } else {
                let name = path.strip_prefix('/').unwrap_or(path);
                if name.contains('/') {
                    return Err(Errno::ENOENT.into());
                }
                let kind = Self::kind_for(name).ok_or(Errno::ENOENT)?;
                if !readable || (writable && !matches!(kind, Kind::Hostname)) {
                    return Err(Errno::EACCES.into());
                }
                kind
            };
            self.files.insert(file_id, OpenFile { kind, cursor: 0 });
            Ok(Opened::File(file_id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let (kind, cursor) = {
                let open = self.files.get(&file).ok_or(Errno::EBADF)?;
                (open.kind, open.cursor)
            };
            let data = self.content(kind)?;
            let remaining = data.get(cursor..).unwrap_or(&[]);
            let length = remaining.len().min(buffer.len());
            let target = buffer.get_mut(..length).ok_or(Errno::EINVAL)?;
            target.copy_from_slice(remaining.get(..length).unwrap_or(&[]));
            if let Some(open) = self.files.get_mut(&file) {
                open.cursor += length;
            }
            Ok(length)
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let kind = self.files.get(&file).ok_or(Errno::EBADF)?.kind;
            if !matches!(kind, Kind::Hostname) {
                return Err(Errno::EACCES.into());
            }
            let value = raven_sysd::sanitize_hostname(buffer).ok_or(Errno::EINVAL)?;
            self.hostname = value;
            Ok(buffer.len())
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files.remove(&file).map(|_| ()).ok_or(Errno::EBADF)?;
            Ok(())
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let kind = self.files.get(&file).ok_or(Errno::EBADF)?.kind;
            let (mode, size) = match kind {
                Kind::Directory => (S_IFDIR | 0o555, 0),
                Kind::Hostname => (S_IFREG | 0o644, self.hostname.len() as u64 + 1),
                _ => (S_IFREG | 0o444, 0),
            };
            Ok(Stat {
                ino: file.wrapping_add(1),
                size,
                mode,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("sys") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("sysd: {}", error.description());
                return 1;
            }
        };
        log_started("sysd");
        let mut handler = SysHandler::new();
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
