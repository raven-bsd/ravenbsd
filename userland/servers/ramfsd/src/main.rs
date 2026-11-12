// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `ramfs:` in-memory writable filesystem server.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

mod handler;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::clock;
    use raven_rt::entry;
    use raven_scheme::Server;

    use crate::handler::RamfsHandler;

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;

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
        let mut handler = RamfsHandler::new(now_seconds);
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            if packet.packet.opcode == OPEN_OPCODE
                && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
            {
                handler.expect_open(packet.packet.file_id, path);
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
