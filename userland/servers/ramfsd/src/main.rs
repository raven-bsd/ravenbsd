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

    /// Parsed service arguments.
    struct Config {
        scheme: &'static str,
        root_mode: u16,
    }

    /// Parse `--scheme <name>` and `--root-mode <octal>`; the defaults are `ramfs` and 0755.
    fn parse_config(args: &raven_rt::Args) -> Result<Config, ()> {
        let mut scheme = "ramfs";
        let mut root_mode = 0o755u16;
        let mut arguments = args.iter().skip(1);
        while let Some(argument) = arguments.next() {
            match argument {
                "--scheme" => {
                    scheme = arguments.next().ok_or(())?;
                    if scheme.is_empty() {
                        return Err(());
                    }
                }
                "--root-mode" => {
                    let value = arguments.next().ok_or(())?;
                    root_mode = u16::from_str_radix(value, 8).map_err(|_| ())?;
                    if root_mode & !0o7777 != 0 {
                        return Err(());
                    }
                }
                _ => return Err(()),
            }
        }
        Ok(Config { scheme, root_mode })
    }

    fn main(args: raven_rt::Args) -> i32 {
        let config = match parse_config(&args) {
            Ok(config) => config,
            Err(()) => {
                raven_rt::eprintln!("ramfsd: usage: ramfsd [--scheme NAME] [--root-mode OCTAL]");
                return 2;
            }
        };
        let mut server = match Server::register(config.scheme) {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("ramfsd: {}", error.description());
                return 1;
            }
        };
        log_started("ramfsd");
        let trace = args.env_get("RAMFSD_TRACE").is_some();
        let mut handler = RamfsHandler::with_trace_root_mode(now_seconds, trace, config.root_mode);
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
