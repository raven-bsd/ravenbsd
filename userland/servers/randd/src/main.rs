// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `rand:` pseudo-random device server.
//!
//! The stream is seeded from the monotonic clock and a stack address and re-seeded from the clock
//! every 64 KiB. It is **not cryptographically secure** until a real entropy source exists.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_randd::Source;
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    struct RandHandler {
        source: Source,
    }

    impl Handler for RandHandler {
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

        fn read(&mut self, _file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            Ok(self.source.fill(buffer, now_ns()))
        }

        fn write(&mut self, _file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            self.source.mix(buffer);
            Ok(buffer.len())
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
        let mut server = match Server::register("rand") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("randd: {}", error.description());
                return 1;
            }
        };
        let stack_address = &server as *const _ as usize;
        let mut handler = RandHandler {
            source: Source::new(now_ns(), stack_address),
        };
        log_started("randd");
        server.run(&mut handler);
    }

    fn log_started(name: &str) {
        let client = raven_scheme::Client::new();
        let path = alloc::format!("log:{name}");
        if let Ok(handle) = client.open(&path, 2) {
            let _ = client.write(handle, 0, b"started\n");
            let _ = client.close(handle);
        }
    }

    fn now_ns() -> u64 {
        match raven_rt::clock_get(clock::MONOTONIC) {
            Ok(time) => u64::try_from(time.tv_sec)
                .unwrap_or(0)
                .saturating_mul(1_000_000_000)
                .saturating_add(u64::try_from(time.tv_nsec).unwrap_or(0)),
            Err(_) => 0,
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
