// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target glue: register `net:`, serve the K30 scheme requests and drive the loopback stack.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::clock;
    use raven_proto_fs::Stat;
    use raven_rt::entry;
    use raven_scheme::{CallReply, Ctx, Handler, HandlerError, HandlerResult, Opened, Server};

    use netd::{Loopback, Netd};

    entry!(main);

    const MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];

    struct NetHandler {
        netd: Netd<Loopback>,
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let now = now_ms();
        let mut handler = NetHandler {
            netd: Netd::new(Loopback::new(MAC), MAC, now),
        };
        let mut server = match Server::register("net") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("netd: net: {}", error.description());
                return 1;
            }
        };
        raven_rt::println!("netd: interface lo 127.0.0.1/8");
        raven_rt::println!("netd: ready");
        // S11 RECV has no timeout. A sleeping heartbeat opens and closes the control file so the
        // single server loop also polls stack timers when no client request is pending; clients
        // still retry socket operations that return EAGAIN.
        let _ = raven_rt::thread::spawn(heartbeat, 0, 16 * 1024);
        loop {
            let now = now_ms();
            let _ = handler.netd.poll(now);
            let Some(packet) = server.poll(None) else {
                continue;
            };
            let _ = server.dispatch(packet, &mut handler);
            let now = now_ms();
            let _ = handler.netd.poll(now);
        }
    }

    impl Handler for NetHandler {
        fn open(
            &mut self,
            path: &str,
            _flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let full = if path.is_empty() {
                alloc::string::String::from("net:")
            } else {
                alloc::format!("net:{path}")
            };
            self.netd
                .open(&full)
                .map(Opened::File)
                .map_err(HandlerError::Errno)
        }

        fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            self.netd
                .read(file, offset, buffer)
                .map_err(HandlerError::Errno)
        }

        fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            self.netd
                .write(file, offset, buffer)
                .map_err(HandlerError::Errno)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            self.netd
                .call(file, request, response)
                .map_err(HandlerError::Errno)
        }

        fn call_attach(
            &mut self,
            file: u64,
            request: &[u8],
            response: &mut [u8],
        ) -> HandlerResult<CallReply> {
            let written = self
                .netd
                .call(file, request, response)
                .map_err(HandlerError::Errno)?;
            Ok(CallReply::new(written))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.netd.close(file).map_err(HandlerError::Errno)
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            self.netd.dup(file).map_err(HandlerError::Errno)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            self.netd.fstat(file).map_err(HandlerError::Errno)
        }
    }

    fn heartbeat(_: usize) {
        loop {
            let _ = raven_rt::nanosleep(10_000_000);
            let client = raven_scheme::Client::new();
            if let Ok(handle) = client.open("net:", 1) {
                let _ = client.close(handle);
            }
        }
    }

    fn now_ms() -> i64 {
        let time = raven_rt::clock_get(clock::MONOTONIC).unwrap_or_default();
        time.tv_sec
            .saturating_mul(1_000)
            .saturating_add(time.tv_nsec / 1_000_000)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
