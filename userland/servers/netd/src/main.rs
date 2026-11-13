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
    use alloc::collections::BTreeMap;
    use alloc::rc::Rc;
    use alloc::string::String;
    use core::cell::Cell;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, clock};
    use raven_proto_fs::Stat;
    use raven_proto_net::socket::{Call, Response, SocketRequest};
    use raven_rt::entry;
    use raven_rt::event::{EventQueue, EvqEvent, READABLE, event_create, event_signal};
    use raven_scheme::{
        CallReply, Ctx, Handler, HandlerError, HandlerResult, Kernel, Opened, RtKernel, Server,
    };

    use netd::{Loopback, NetEvent, Netd};

    entry!(main);

    const MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];

    struct NetHandler {
        netd: Netd<Loopback>,
        events: BTreeMap<u64, Handle>,
        nic_retry_ms: i64,
    }

    fn main(args: raven_rt::Args) -> i32 {
        let now = now_ms();
        let require_nic = args.get(1) == Some("nic");
        let netd = Netd::new(Loopback::new(MAC), MAC, now);
        let mut handler = NetHandler {
            netd,
            events: BTreeMap::new(),
            nic_retry_ms: now,
        };
        if require_nic {
            if let Err(error) = handler.try_nic(now) {
                raven_rt::eprintln!("netd: nic: {error}");
                return 1;
            }
        } else {
            let _ = handler.try_nic(now);
        }
        let registered = Rc::new(Cell::new(None));
        let kernel = RecordingKernel {
            inner: RtKernel,
            registered: Rc::clone(&registered),
        };
        let mut server = match Server::register_with(kernel, "net") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("netd: net: {}", error.description());
                return 1;
            }
        };
        let Some(server_handle) = registered.get() else {
            raven_rt::eprintln!("netd: scheme handle missing");
            return 1;
        };
        let queue = match EventQueue::create() {
            Ok(queue) => queue,
            Err(error) => {
                raven_rt::eprintln!("netd: event queue: {}", error.description());
                return 1;
            }
        };
        if let Err(error) = queue.add(server_handle, READABLE, 1) {
            raven_rt::eprintln!("netd: event add: {}", error.description());
            return 1;
        }
        raven_rt::println!("netd: interface lo 127.0.0.1/8");
        raven_rt::println!("netd: ready");
        let mut events = [EvqEvent::default(); 4];
        loop {
            let now = now_ms();
            if !handler.netd.has_nic() && now >= handler.nic_retry_ms {
                let _ = handler.try_nic(now);
                handler.nic_retry_ms = now.saturating_add(1_000);
            }
            let ready = handler.netd.poll(now);
            handler.signal_events(&ready);
            // No stack timer pending: block on the scheme-server source and its client events.
            // The one-second cap is only a safety net for a missed wakeup, not a heartbeat.
            let timeout_ns = handler
                .netd
                .next_poll_delay_ms(now)
                .map(|ms| ms.clamp(1, 1000).saturating_mul(1_000_000))
                .unwrap_or(1_000_000_000);
            let count = queue.wait(&mut events, timeout_ns).unwrap_or(0);
            if count == 0 {
                continue;
            }
            // The scheme-server source is readable only when its queue is non-empty; the
            // blocking RECV below therefore has a request ready.
            let Some(packet) = server.poll(None) else {
                continue;
            };
            let _ = server.dispatch(packet, &mut handler);
            let now = now_ms();
            let ready = handler.netd.poll(now);
            handler.signal_events(&ready);
        }
    }

    impl NetHandler {
        fn try_nic(&mut self, now: i64) -> Result<(), String> {
            if self.netd.has_nic() {
                return Ok(());
            }
            let config = netd::nic::attach(&mut self.netd, now)?;
            raven_rt::println!(
                "netd: interface eth0 {}.{}.{}.{}/{}",
                config.addr[0],
                config.addr[1],
                config.addr[2],
                config.addr[3],
                config.prefix
            );
            Ok(())
        }

        fn signal_events(&mut self, ready: &[NetEvent]) {
            for ready in ready {
                let socket = match ready.event {
                    raven_netcore::Event::Readable(socket)
                    | raven_netcore::Event::Writable(socket)
                    | raven_netcore::Event::Closed(socket)
                    | raven_netcore::Event::Error(socket, _) => socket,
                    raven_netcore::Event::Accepted { listener, .. } => listener,
                };
                for file in self.netd.files_for_socket(ready.interface, socket) {
                    if let Some(event) = self.events.get(&file) {
                        let _ = event_signal(*event, 1);
                    }
                }
            }
        }
    }

    #[derive(Clone)]
    struct RecordingKernel {
        inner: RtKernel,
        registered: Rc<Cell<Option<Handle>>>,
    }

    impl Kernel for RecordingKernel {
        fn scheme_register(&self, name: &str) -> Result<(), Errno> {
            let handle = raven_rt::event::scheme_register_handle(name)?;
            self.registered.set(Some(handle));
            Ok(())
        }

        fn open(&self, path: &str, flags: u32) -> Result<Handle, Errno> {
            self.inner.open(path, flags)
        }

        fn read(&self, file: Handle, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
            self.inner.read(file, offset, buffer)
        }

        fn write(&self, file: Handle, offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
            self.inner.write(file, offset, buffer)
        }

        fn close(&self, file: Handle) -> Result<(), Errno> {
            self.inner.close(file)
        }

        fn dup(&self, file: Handle, rights: u32) -> Result<Handle, Errno> {
            self.inner.dup(file, rights)
        }

        fn fstat(&self, file: Handle, out: &mut [u8; 64]) -> Result<(), Errno> {
            self.inner.fstat(file, out)
        }

        fn call(&self, file: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
            self.inner.call(file, request, response)
        }

        fn recv(&self, packet: &mut [u8]) -> Result<Handle, Errno> {
            self.inner.recv(packet)
        }

        fn recv_timeout(
            &self,
            packet: &mut [u8],
            timeout_ms: u64,
        ) -> Result<Option<Handle>, Errno> {
            self.inner.recv_timeout(packet, timeout_ms)
        }

        fn send(&self, request: Handle, packet: &[u8]) -> Result<(), Errno> {
            self.inner.send(request, packet)
        }

        fn send_attach(
            &self,
            request: Handle,
            packet: &[u8],
            attachments: &[raven_scheme::Attachment],
        ) -> Result<(), Errno> {
            self.inner.send_attach(request, packet, attachments)
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
            if let Ok(SocketRequest::Call(Call::Event)) = SocketRequest::decode(request) {
                if self.netd.socket_id(file).is_none() && self.netd.socket_interface(file).is_none()
                {
                    return Err(HandlerError::Errno(Errno::EINVAL));
                }
                let event = event_create().map_err(HandlerError::Errno)?;
                if let Some(previous) = self.events.insert(file, event) {
                    let _ = raven_rt::handle_close(previous);
                }
                let mut data = [0u8; 6];
                Response::Ok
                    .encode(&mut data)
                    .map_err(|_| HandlerError::Errno(Errno::E2BIG))?;
                let mut reply = CallReply::with_data(&data)?;
                reply.attach(event, 0)?;
                return Ok(reply);
            }
            let written = self
                .netd
                .call(file, request, response)
                .map_err(HandlerError::Errno)?;
            Ok(CallReply::new(written))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            if let Some(event) = self.events.remove(&file) {
                let _ = raven_rt::handle_close(event);
            }
            self.netd.close(file).map_err(HandlerError::Errno)
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            self.netd.dup(file).map_err(HandlerError::Errno)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            self.netd.fstat(file).map_err(HandlerError::Errno)
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
