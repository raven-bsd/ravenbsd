// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `null:` device server.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    struct NullHandler;

    impl Handler for NullHandler {
        fn open(
            &mut self,
            _path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            if flags & 4 != 0 {
                return Err(Errno::EINVAL.into());
            }
            Ok(Opened::File(0))
        }

        fn read(&mut self, _file: u64, _offset: u64, _buffer: &mut [u8]) -> HandlerResult<usize> {
            Ok(raven_nulld::read_len())
        }

        fn write(&mut self, _file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            Ok(raven_nulld::discard_write(buffer))
        }

        fn close(&mut self, _file: u64) -> HandlerResult<()> {
            Ok(())
        }

        fn fstat(&mut self, _file: u64) -> HandlerResult<Stat> {
            Ok(Stat {
                mode: S_IFCHR | 0o666,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("null") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("nulld: {}", error.description());
                return 1;
            }
        };
        log_started("nulld");
        server.run(&mut NullHandler);
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
