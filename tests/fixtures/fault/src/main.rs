// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S71 fault-injection fixture: server death during an in-flight request, and the idle /
//! during-restart kill matrix.
//!
//! `worker` registers the `frt:` scheme. A read on `frt:/poison` terminates the worker inside the
//! handler, before the scheme server can send a reply, so the client's `READ` is genuinely in
//! flight when the process dies. `client` issues that read, asserts the kernel fails it with the
//! defined `EIO`/`EPIPE`, observes the `svc:/frt.gen` generation change, keeps the stale handle
//! failing promptly and reopens through the bounded `raven_scheme::service::reopen` path.
//!
//! `restart` drives the same worker through the `svc:` control scheme: a successful read (after
//! reply), an explicit `RESTART` (during-restart kill point), the dead-handle error, a bounded
//! reopen, then `STOP`/`START` (idle kill point) and a second reopen.

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

    /// `OPEN_READ` wire flag.
    const READ: u32 = 1;
    /// How many 10 ms polls a wait allows.
    const POLLS: usize = 1000;
    /// Poll interval in nanoseconds.
    const POLL_NS: u64 = 10_000_000;
    /// Path whose first read kills the worker mid-request.
    const POISON: &str = "/poison";

    struct FaultScheme {
        poison: bool,
    }

    impl Handler for FaultScheme {
        fn open(
            &mut self,
            path: &str,
            _flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            self.poison = path == POISON;
            Ok(Opened::File(1))
        }

        fn read(&mut self, _file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if self.poison {
                // Die inside dispatch: the client's READ has no reply yet, so the kernel has to
                // fail the in-flight request. svcd sees the non-zero exit and restarts the worker.
                raven_rt::exit(1);
            }
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
            Some("client") => match reconnect() {
                Ok(()) => {
                    raven_rt::println!("fault-reconnect: ok");
                    0
                }
                Err(message) => {
                    raven_rt::eprintln!("fault-reconnect: {message}");
                    1
                }
            },
            Some("restart") => match restart_matrix() {
                Ok(()) => {
                    raven_rt::println!("fault-restart: ok");
                    0
                }
                Err(message) => {
                    raven_rt::eprintln!("fault-restart: {message}");
                    1
                }
            },
            _ => {
                raven_rt::eprintln!("fault: expected worker, client or restart");
                2
            }
        }
    }

    fn worker() -> i32 {
        let Ok(mut server) = Server::register("frt") else {
            return 1;
        };
        let mut handler = FaultScheme { poison: false };
        server.run(&mut handler)
    }

    /// In-flight death: the client's first read is pending when the worker exits.
    fn reconnect() -> Result<(), String> {
        let client = Client::new();
        let generation0 = service_generation(&client, "frt")
            .map_err(|error| format!("gen: {}", error.description()))?;
        raven_rt::println!("fault-reconnect: gen0={generation0}");

        let stale = open(&client, "frt:/poison")?;
        let mut buffer = [0u8; 8];
        match client.read(stale, 0, &mut buffer) {
            Err(error) if error == Errno::EIO || error == Errno::EPIPE => {
                raven_rt::println!("fault-reconnect: inflight:{}", error.0);
            }
            Err(error) => return Err(format!("inflight: unexpected {}", error.description())),
            Ok(_) => return Err("in-flight read unexpectedly succeeded".to_string()),
        }

        let generation1 = wait_running(&client, generation0)?;
        raven_rt::println!("fault-reconnect: gen1={generation1}");

        // The stale handle must keep failing promptly, never hang on a dead server.
        match client.read(stale, 0, &mut buffer) {
            Err(error) => raven_rt::println!("fault-reconnect: stale:{}", error.0),
            Ok(_) => return Err("stale handle read unexpectedly succeeded".to_string()),
        }
        let _ = client.close(stale);

        let reopened = reopen(&client, "frt:", "frt", generation0, &sleep)
            .map_err(|error| format!("reopen: {}", error.description()))?;
        read_ok(&client, reopened)?;
        let _ = client.close(reopened);
        if generation1 <= generation0 {
            return Err("generation did not advance".to_string());
        }
        raven_rt::println!("fault-reconnect: reopened");
        Ok(())
    }

    /// After-reply, during-restart and idle stop/start kill points on the same worker.
    fn restart_matrix() -> Result<(), String> {
        let client = Client::new();
        let generation0 = service_generation(&client, "frt")
            .map_err(|error| format!("gen: {}", error.description()))?;
        raven_rt::println!("fault-restart: gen0={generation0}");

        // After reply: a successful read leaves a live handle.
        let before = open(&client, "frt:")?;
        read_ok(&client, before)?;
        raven_rt::println!("fault-restart: read-before=ok");

        // During restart: stop and start through the control scheme.
        control(&client, Op::Restart)?;
        let generation1 = wait_running(&client, generation0)?;
        raven_rt::println!("fault-restart: restarted");
        raven_rt::println!("fault-restart: gen1={generation1}");

        // The handle opened before the restart is dead and must fail promptly.
        match client.read(before, 0, &mut [0u8; 8]) {
            Err(error) => raven_rt::println!("fault-restart: stale:{}", error.0),
            Ok(_) => return Err("stale handle read unexpectedly succeeded".to_string()),
        }
        let _ = client.close(before);
        let reopened = reopen(&client, "frt:", "frt", generation0, &sleep)
            .map_err(|error| format!("reopen1: {}", error.description()))?;
        read_ok(&client, reopened)?;
        let _ = client.close(reopened);
        raven_rt::println!("fault-restart: reopen1");

        // Idle: stop the service, confirm it is down, then start it again.
        control(&client, Op::Stop)?;
        wait_stopped(&client)?;
        raven_rt::println!("fault-restart: stopped");

        control(&client, Op::Start)?;
        let generation2 = wait_running(&client, generation1)?;
        raven_rt::println!("fault-restart: started");
        raven_rt::println!("fault-restart: gen2={generation2}");

        let reopened = reopen(&client, "frt:", "frt", generation1, &sleep)
            .map_err(|error| format!("reopen2: {}", error.description()))?;
        read_ok(&client, reopened)?;
        let _ = client.close(reopened);
        raven_rt::println!("fault-restart: reopen2");
        Ok(())
    }

    fn control(client: &Client, op: Op) -> Result<(), String> {
        let handle = open(client, "svc:/frt")?;
        let mut response = [0u8; 1];
        let result = client
            .call(handle, &[op.as_u8()], &mut response)
            .map(|_| ())
            .map_err(|error| format!("control: {}", error.description()));
        let _ = client.close(handle);
        result
    }

    fn wait_running(client: &Client, previous: u64) -> Result<u64, String> {
        for _ in 0..POLLS {
            let generation = service_generation(client, "frt")
                .map_err(|error| format!("gen: {}", error.description()))?;
            let running = service_running(client, "frt")
                .map_err(|error| format!("state: {}", error.description()))?;
            if running && generation != previous {
                return Ok(generation);
            }
            let _ = raven_rt::nanosleep(POLL_NS);
        }
        Err("worker did not come back".to_string())
    }

    fn wait_stopped(client: &Client) -> Result<(), String> {
        for _ in 0..POLLS {
            let running = service_running(client, "frt")
                .map_err(|error| format!("state: {}", error.description()))?;
            if !running {
                return Ok(());
            }
            let _ = raven_rt::nanosleep(POLL_NS);
        }
        Err("worker did not stop".to_string())
    }

    fn read_ok(client: &Client, handle: Handle) -> Result<(), String> {
        let mut buffer = [0u8; 8];
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("read: {}", error.description()))?;
        if read == 0 {
            return Err("read returned EOF".to_string());
        }
        Ok(())
    }

    fn open(client: &Client, path: &str) -> Result<Handle, String> {
        client
            .open(path, READ)
            .map_err(|error: Errno| format!("open {path}: {}", error.description()))
    }

    fn sleep(nanos: u64) {
        let _ = raven_rt::nanosleep(nanos);
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
