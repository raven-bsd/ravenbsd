// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S70 service restart fixture.
//!
//! `worker` registers the `rst:` scheme and answers reads with `r` bytes. `client` reads the
//! service generation, restarts the worker through `svc:`, observes the dead-handle error, then
//! reopens the scheme through the bounded reconnect helper and verifies the generation advanced.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_svc::Op;
    use raven_rt::entry;
    use raven_scheme::service::{reopen, service_generation, service_running};
    use raven_scheme::{Client, Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    const READ: u32 = 1;
    const POLLS: usize = 500;
    const POLL_NS: u64 = 10_000_000;

    struct RstScheme;

    impl Handler for RstScheme {
        fn open(
            &mut self,
            _path: &str,
            _flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            Ok(Opened::File(1))
        }

        fn read(&mut self, _file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            for byte in buffer.iter_mut() {
                *byte = b'r';
            }
            Ok(buffer.len())
        }

        fn close(&mut self, _file: u64) -> HandlerResult<()> {
            Ok(())
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        match args.get(0) {
            Some("worker") => worker(),
            Some("client") => match client() {
                Ok(()) => {
                    raven_rt::println!("service-restart: ok");
                    0
                }
                Err(message) => {
                    raven_rt::eprintln!("service-restart: {message}");
                    1
                }
            },
            _ => {
                raven_rt::eprintln!("service-restart: expected worker or client");
                2
            }
        }
    }

    fn worker() -> i32 {
        let Ok(mut server) = Server::register("rst") else {
            return 1;
        };
        let mut handler = RstScheme;
        server.run(&mut handler)
    }

    fn client() -> Result<(), String> {
        let client = Client::new();
        let generation0 = service_generation(&client, "rst")
            .map_err(|error| format!("gen: {}", error.description()))?;
        raven_rt::println!("service-restart: gen0={generation0}");

        let stale = open(&client, "rst:")?;
        let mut buffer = [0u8; 8];
        let read = client
            .read(stale, 0, &mut buffer)
            .map_err(|error| format!("initial read: {}", error.description()))?;
        if read == 0 {
            return Err("initial read returned EOF".to_string());
        }

        restart(&client)?;
        let generation1 = wait_running(&client, generation0)?;

        match client.read(stale, 0, &mut buffer) {
            Err(error) => {
                raven_rt::println!("service-restart: stale:{}", error.0);
            }
            Ok(_) => return Err("stale handle read unexpectedly succeeded".to_string()),
        }
        let _ = client.close(stale);

        let reopened = reopen(&client, "rst:", "rst", generation0, &|nanos| {
            let _ = raven_rt::nanosleep(nanos);
        })
        .map_err(|error| format!("reopen: {}", error.description()))?;
        let mut fresh = [0u8; 8];
        let read = client
            .read(reopened, 0, &mut fresh)
            .map_err(|error| format!("reopen read: {}", error.description()))?;
        let _ = client.close(reopened);
        if read == 0 {
            return Err("reopen read returned EOF".to_string());
        }
        if generation1 <= generation0 {
            return Err("generation did not advance".to_string());
        }
        raven_rt::println!("service-restart: reopened");
        raven_rt::println!("service-restart: gen1={generation1}");
        Ok(())
    }

    fn restart(client: &Client) -> Result<(), String> {
        let handle = open(client, "svc:/rst")?;
        let mut response = [0u8; 1];
        let result = client
            .call(handle, &[Op::Restart.as_u8()], &mut response)
            .map(|_| ())
            .map_err(|error| format!("restart: {}", error.description()));
        let _ = client.close(handle);
        result
    }

    fn wait_running(client: &Client, previous: u64) -> Result<u64, String> {
        for _ in 0..POLLS {
            let generation = service_generation(client, "rst")
                .map_err(|error| format!("gen: {}", error.description()))?;
            let running = service_running(client, "rst")
                .map_err(|error| format!("state: {}", error.description()))?;
            if running && generation != previous {
                return Ok(generation);
            }
            let _ = raven_rt::nanosleep(POLL_NS);
        }
        Err("worker did not come back".to_string())
    }

    fn open(client: &Client, path: &str) -> Result<Handle, String> {
        client
            .open(path, READ)
            .map_err(|error: Errno| format!("open {path}: {}", error.description()))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
