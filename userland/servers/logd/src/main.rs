// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `log:` system log server.

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
    use raven_logd::{Ring, format_record};
    use raven_proto_fs::{S_IFREG, Stat};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;
    const OPEN_CREATE: u32 = 4;

    struct OpenFile {
        name: String,
        readable: bool,
        writable: bool,
        cursor: u64,
    }

    struct LogHandler {
        ring: Ring,
        files: BTreeMap<u64, OpenFile>,
        pending: Option<(u64, String)>,
        console: bool,
    }

    impl Handler for LogHandler {
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
            if !readable && !writable {
                return Err(Errno::EACCES.into());
            }
            let name = path.strip_prefix('/').unwrap_or(path);
            if name.contains('/') {
                return Err(Errno::EINVAL.into());
            }
            let name = if name.is_empty() { "log" } else { name }.to_string();
            self.files.insert(
                file_id,
                OpenFile {
                    name,
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
                return Err(Errno::EACCES.into());
            }
            let (length, cursor) = self.ring.read(open.cursor, buffer);
            open.cursor = cursor;
            Ok(length)
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let name = {
                let open = self.files.get(&file).ok_or(Errno::EBADF)?;
                if !open.writable {
                    return Err(Errno::EACCES.into());
                }
                open.name.clone()
            };
            let (seconds, millis) = timestamp();
            let record = format_record(seconds, millis, &name, buffer);
            self.ring.push(&record);
            if self.console {
                let _ = raven_rt::debug_write(&record);
            }
            Ok(buffer.len())
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files.remove(&file).map(|_| ()).ok_or(Errno::EBADF)?;
            Ok(())
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            self.files.get(&file).ok_or(Errno::EBADF)?;
            Ok(Stat {
                ino: file.wrapping_add(1),
                mode: S_IFREG | 0o644,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let console = !args.iter().skip(1).any(|argument| argument == "console=0");
        let mut server = match Server::register("log") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("logd: {}", error.description());
                return 1;
            }
        };
        let mut handler = LogHandler {
            ring: Ring::new(),
            files: BTreeMap::new(),
            pending: None,
            console,
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

    fn timestamp() -> (u64, u32) {
        let time = raven_rt::clock_get(clock::REALTIME)
            .or_else(|_| raven_rt::clock_get(clock::MONOTONIC))
            .unwrap_or_default();
        let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
        let millis = u32::try_from(time.tv_nsec / 1_000_000).unwrap_or(0);
        (seconds, millis)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
