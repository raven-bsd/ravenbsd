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
    use alloc::collections::{BTreeMap, BTreeSet};
    use alloc::rc::Rc;
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::cell::Cell;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, clock};
    use raven_proto_fs::{O_RDWR, Stat};
    use raven_proto_net::socket::{Call, Response, SocketRequest};
    use raven_rt::entry;
    use raven_rt::event::{EventQueue, EvqEvent, READABLE, event_create, event_signal};
    use raven_scheme::{
        CallReply, Client, Ctx, Handler, HandlerError, HandlerResult, Kernel, Opened, RtKernel,
        Server,
    };

    use netd::network::{LoadError, NetworkConfig, NetworkMode};
    use netd::nic::NicMode;
    use netd::{FilterPolicy, LoadError as FilterLoadError, Loopback, NetEvent, Netd};

    entry!(main);

    const MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
    const OPEN_WRITE: u32 = 2;
    /// Doorbells kept per file (one per process that polls the shared descriptor).
    const MAX_DOORBELLS_PER_FILE: usize = 16;

    struct NetHandler {
        netd: Netd<Loopback>,
        /// Readiness doorbells per open file. A file is shared by every process that holds a
        /// handle to it, so all holders are rung.
        events: BTreeMap<u64, Vec<Handle>>,
        /// Files whose level is re-checked after the next stack poll (accepted backlog).
        rearm: BTreeSet<u64>,
        nic_retry_ms: i64,
        nic_mode: NicMode,
        printed_eth0: bool,
        release_dhcp: bool,
        release_ping: bool,
        release_unavailable: bool,
        network: Option<NetworkConfig>,
        network_ready: bool,
        network_deadline_ms: i64,
        hostname: Option<String>,
        hostname_applied: bool,
        static_mode: bool,
        /// `/etc/pf.conf` policy shared by every interface.
        pf: Option<FilterPolicy>,
    }

    fn main(args: raven_rt::Args) -> i32 {
        let now = now_ms();
        let require_nic = args.get(1) == Some("nic");
        // Fail closed: a present but malformed ruleset stops netd instead of starting
        // unfiltered. An absent file keeps the historical no-filter behaviour.
        let pf = match load_pf_policy() {
            Ok(policy) => policy,
            Err(error) => {
                raven_rt::eprintln!("netd: pf config: {error}; refusing to start");
                return 1;
            }
        };
        match &pf {
            Some(policy) => raven_rt::println!(
                "netd: pf loaded rules={} nat={}",
                policy.rules(),
                policy.nat_rules()
            ),
            None => raven_rt::println!("netd: pf absent"),
        }
        let netd = Netd::with_filter(Loopback::new(MAC), MAC, now, pf.clone());
        let mut handler = NetHandler {
            netd,
            events: BTreeMap::new(),
            rearm: BTreeSet::new(),
            nic_retry_ms: now,
            nic_mode: NicMode::select(&args),
            printed_eth0: false,
            release_dhcp: false,
            release_ping: false,
            release_unavailable: false,
            network: None,
            network_ready: false,
            network_deadline_ms: now.saturating_add(10_000),
            hostname: None,
            hostname_applied: false,
            static_mode: false,
            pf,
        };
        handler.load_network(now);
        // A `nic:` driver may register a moment after svcd launches netd. Register `net:` for
        // loopback first and retry the NIC from the main loop so a late driver never fails netd
        // or delays getty. The service manifest still waits for `scheme:nic` where available.
        if let Err(error) = handler.try_nic(now)
            && require_nic
        {
            raven_rt::eprintln!("netd: nic: {error}; waiting for the driver");
        }
        handler.publish_and_feed();
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
        let mut tcp_debug_ms = 0i64;
        loop {
            let now = now_ms();
            if !handler.netd.has_nic() && now >= handler.nic_retry_ms {
                let _ = handler.try_nic(now);
                handler.nic_retry_ms = now.saturating_add(1_000);
            }
            let ready = handler.netd.poll(now);
            handler.signal_events(&ready);
            handler.publish_and_feed();
            if now >= tcp_debug_ms.saturating_add(1_000) {
                let c = raven_netcore::debug_counters();
                raven_rt::println!(
                    "netd: tcpdbg send={} se={} runs={} unsent={} ok={} ea={}",
                    c[0],
                    c[1],
                    c[2],
                    c[3],
                    c[4],
                    c[5]
                );
                tcp_debug_ms = now;
            }
            for line in handler.netd.take_filter_log() {
                raven_rt::println!("{line}");
            }
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
            handler.publish_and_feed();
            if now >= tcp_debug_ms.saturating_add(1_000) {
                let c = raven_netcore::debug_counters();
                raven_rt::println!(
                    "netd: tcpdbg send={} se={} runs={} unsent={} ok={} ea={}",
                    c[0],
                    c[1],
                    c[2],
                    c[3],
                    c[4],
                    c[5]
                );
                tcp_debug_ms = now;
            }
            for line in handler.netd.take_filter_log() {
                raven_rt::println!("{line}");
            }
        }
    }

    /// Load `/etc/pf.conf`, retrying while the file service starts. Missing means no filter;
    /// malformed means the caller must exit without starting the stack.
    fn load_pf_policy() -> Result<Option<FilterPolicy>, String> {
        let deadline = now_ms().saturating_add(10_000);
        loop {
            match netd::load_pf() {
                Ok(Some(ruleset)) => return Ok(Some(FilterPolicy::new(ruleset))),
                Ok(None) => return Ok(None),
                Err(FilterLoadError::NotReady) if now_ms() < deadline => {
                    let _ = raven_rt::nanosleep(5_000_000);
                }
                Err(error) => return Err(error.message()),
            }
        }
    }

    impl NetHandler {
        fn try_nic(&mut self, now: i64) -> Result<(), String> {
            if self.netd.has_nic() {
                return Ok(());
            }
            if !self.load_network(now) {
                return Ok(());
            }
            let static_config = self.network.as_ref().and_then(NetworkConfig::static_config);
            let _config = netd::nic::attach(
                &mut self.netd,
                now,
                self.nic_mode,
                static_config,
                self.pf.clone(),
            )?;
            Ok(())
        }

        /// Read and validate the network drop-in once. Returns false only while the file
        /// service is still starting, so the caller can retry without attaching a wrong mode.
        fn load_network(&mut self, now: i64) -> bool {
            if self.network_ready {
                return true;
            }
            match netd::network::load() {
                Ok(Some(config)) => {
                    self.static_mode = config.mode == NetworkMode::Static;
                    self.hostname = config.hostname.clone();
                    match config.mode {
                        NetworkMode::Static => raven_rt::println!(
                            "netd: network config mode=static hostname={}",
                            config.hostname.as_deref().unwrap_or("(image default)")
                        ),
                        NetworkMode::Dhcp => raven_rt::println!(
                            "netd: network config mode=dhcp hostname={}",
                            config.hostname.as_deref().unwrap_or("(image default)")
                        ),
                    }
                    self.network = Some(config);
                    self.network_ready = true;
                    true
                }
                Ok(None) => {
                    self.network_ready = true;
                    true
                }
                Err(LoadError::NotReady) => {
                    if now >= self.network_deadline_ms {
                        self.network_ready = true;
                        true
                    } else {
                        false
                    }
                }
                Err(error @ LoadError::Invalid(_)) => {
                    raven_rt::eprintln!(
                        "netd: network config: {}; using image default",
                        error.message()
                    );
                    self.network_ready = true;
                    true
                }
            }
        }

        fn apply_hostname(&mut self) {
            if self.hostname_applied {
                return;
            }
            let Some(name) = self.hostname.clone() else {
                return;
            };
            if Self::write_hostname(&name) {
                raven_rt::println!("netd: hostname {name}");
                self.hostname_applied = true;
            }
        }

        fn write_hostname(name: &str) -> bool {
            let Ok(mut file) = raven_rt::fs::open("sys:/hostname", O_RDWR) else {
                return false;
            };
            let written = file.write(name.as_bytes()).is_ok();
            let _ = file.close();
            written
        }

        fn publish_and_feed(&mut self) {
            self.apply_hostname();
            if !self.printed_eth0
                && let Some(config) = self.netd.published_nic()
            {
                raven_rt::println!(
                    "netd: interface eth0 {}.{}.{}.{}/{}",
                    config.addr[0],
                    config.addr[1],
                    config.addr[2],
                    config.addr[3],
                    config.prefix
                );
                self.printed_eth0 = true;
            }
            self.feed_dhcp();
            self.feed_ping();
        }

        fn feed_dhcp(&mut self) {
            if self.static_mode || self.release_dhcp || self.release_unavailable {
                return;
            }
            if self.netd.published_nic().is_none() {
                return;
            }
            let Some(lease) = self.netd.dhcp_lease() else {
                return;
            };
            let Some(line) = netd::release_dhcp_line(&lease) else {
                return;
            };
            if Self::write_release(&line) {
                self.release_dhcp = true;
            } else {
                self.release_unavailable = true;
            }
        }

        fn feed_ping(&mut self) {
            if self.release_ping || self.release_unavailable {
                return;
            }
            let Some(ping) = self.netd.take_release_ping() else {
                return;
            };
            let line = netd::release_ping_line(ping);
            if Self::write_release(&line) {
                self.release_ping = true;
            } else {
                self.release_unavailable = true;
            }
        }

        fn write_release(line: &str) -> bool {
            let client = Client::new();
            let Ok(handle) = client.open("nic:release", OPEN_WRITE) else {
                return false;
            };
            let written = client.write(handle, 0, line.as_bytes()).is_ok();
            let _ = client.close(handle);
            written
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
                    self.ring(file);
                }
            }
            // An accept can leave a second connection in the backlog and the stack raises no new
            // edge for it. A read deliberately does not re-arm: the doorbell latch is cleared only
            // by EVQ_WAIT, so a signal after a partial read outlives the read and an inherited
            // socket sees a spurious ready later. The live query (K244) is the verdict; the edge
            // is only the wake-up.
            let touched = core::mem::take(&mut self.rearm);
            for file in touched {
                if self
                    .netd
                    .socket_readiness(file)
                    .is_ok_and(|ready| ready.readable)
                {
                    self.ring(file);
                }
            }
        }

        /// Ring every doorbell attached to `file`.
        fn ring(&self, file: u64) {
            if let Some(events) = self.events.get(&file) {
                for event in events {
                    let _ = event_signal(*event, 1);
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
            if let Ok(SocketRequest::Call(Call::Accept)) = SocketRequest::decode(request) {
                self.rearm.insert(file);
            }
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
                // The transition that made the socket ready may have happened before this
                // doorbell existed (an accepted socket is opened after its data arrived; an
                // inherited descriptor is polled long after). The live query is the truth, not
                // the edge: ring now if there is something to collect.
                if self
                    .netd
                    .socket_readiness(file)
                    .is_ok_and(|ready| ready.readable || ready.writable)
                {
                    let _ = event_signal(event, 1);
                }
                let doorbells = self.events.entry(file).or_default();
                if doorbells.len() >= MAX_DOORBELLS_PER_FILE {
                    // Bounded: drop the oldest. Its holder keeps a valid but silent handle.
                    let oldest = doorbells.remove(0);
                    let _ = raven_rt::handle_close(oldest);
                }
                doorbells.push(event);
                let mut data = [0u8; 6];
                Response::Ok
                    .encode(&mut data)
                    .map_err(|_| HandlerError::Errno(Errno::E2BIG))?;
                let mut reply = CallReply::with_data(&data)?;
                reply.attach(event, 0)?;
                return Ok(reply);
            }
            if let Ok(SocketRequest::Call(Call::Accept)) = SocketRequest::decode(request) {
                self.rearm.insert(file);
            }
            let written = self
                .netd
                .call(file, request, response)
                .map_err(HandlerError::Errno)?;
            Ok(CallReply::new(written))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            let last = self.netd.close(file).map_err(HandlerError::Errno)?;
            if last {
                self.rearm.remove(&file);
                for event in self.events.remove(&file).unwrap_or_default() {
                    let _ = raven_rt::handle_close(event);
                }
            }
            Ok(())
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
