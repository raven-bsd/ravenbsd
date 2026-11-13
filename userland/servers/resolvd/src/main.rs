// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD DNS resolver service (`resolv:` scheme).
//!
//! The target reads the static `/etc/resolv.conf`, or applies the live DHCP
//! lease that `netd` exposes on its `net:` control channel.  A static
//! nameserver list wins; otherwise the lease DNS servers are written to the
//! runtime file and used for A/AAAA queries through `netd`'s UDP sockets.
//! The lease thread refreshes from the RFC 2131 renew/rebind/expiry timers and
//! never owns a second network stack.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
    use raven_proto_net::{DHCP_TIMER_UNKNOWN, IfName, IfaceRequest, IfaceResponse};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_scheme::{Client, Ctx, Handler, HandlerResult, OpenTable, Opened, Server};
    use resolvd::config::{self, Config};
    use resolvd::dns::Address as DnsAddress;
    use resolvd::lease::Lease;
    use resolvd::proto::{Address, QTYPE_AAAA, Query, Record, Reply};
    use resolvd::resolver::{Exchange, ResolveError, Resolver};

    entry!(main);

    const OPEN_OPCODE: u16 = 1;
    const READ_WRITE: u32 = 3;
    const MAX_CONF_BYTES: usize = 4096;
    const DEFAULT_STATIC: &str = "initfs:/etc/resolv.conf";
    const DEFAULT_RUNTIME: &str = "file:/etc/resolv.conf";
    const DEFAULT_IFACE: &str = "eth0";
    const DNS_PORT: u16 = 53;
    const LOCAL_PORT: u16 = 5301;
    const TIMEOUT_MS: u32 = 500;
    const ATTEMPTS: u8 = 2;
    const LEASE_WAIT_MS: u64 = 5_000;
    const LEASE_RETRY_MS: u64 = 100;
    const LEASE_BACKOFF_MAX_MS: u64 = 5_000;
    const LEASE_THREAD_STACK: usize = 96 * 1024;

    /// The resolver the scheme handler uses.  Replaced by the lease thread.
    static RESOLVER: Mutex<Option<Resolver>> = Mutex::new(None);
    /// The last applied lease, so the refresh only logs real changes.
    static LAST_LEASE: Mutex<Option<Lease>> = Mutex::new(None);
    /// Interface and runtime path handed to the lease thread.
    static LEASE_PLAN: Mutex<Option<Plan>> = Mutex::new(None);

    struct Plan {
        iface: String,
        conf: String,
    }

    impl Clone for Plan {
        fn clone(&self) -> Self {
            Self {
                iface: self.iface.clone(),
                conf: self.conf.clone(),
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let static_path = args
            .env_get("RESOLVD_STATIC")
            .map(String::from)
            .unwrap_or_else(static_path_default);
        let conf_path = args
            .env_get("RESOLVD_CONF")
            .map(String::from)
            .unwrap_or_else(runtime_path_default);
        let iface = args
            .env_get("RESOLVD_IFACE")
            .map(String::from)
            .unwrap_or_else(|| DEFAULT_IFACE.to_string());

        if let Some(config) = read_static_override(&static_path) {
            raven_rt::println!("resolvd: static nameservers={}", config.servers.len());
            set_resolver(config);
        } else {
            match wait_for_lease(&iface, LEASE_WAIT_MS) {
                Some(lease) => apply_lease(&lease, &iface, &conf_path),
                None => raven_rt::println!("resolvd: lease not ready; retrying in the background"),
            }
            *LEASE_PLAN.lock() = Some(Plan {
                iface: iface.clone(),
                conf: conf_path.clone(),
            });
            if let Err(error) = raven_rt::thread::spawn(lease_worker, 0, LEASE_THREAD_STACK) {
                raven_rt::eprintln!("resolvd: lease thread: {}", error.description());
            }
        }

        let mut handler = HandlerState::new();
        let mut server = match Server::register("resolv") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("resolvd: {}", error.description());
                return 1;
            }
        };
        log_started("resolvd");
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

    fn static_path_default() -> String {
        if root_mounted() {
            "file:/root/etc/resolv.conf".to_string()
        } else {
            DEFAULT_STATIC.to_string()
        }
    }

    fn runtime_path_default() -> String {
        if root_mounted() {
            "file:/root/etc/resolv.conf".to_string()
        } else {
            DEFAULT_RUNTIME.to_string()
        }
    }

    fn root_mounted() -> bool {
        raven_rt::fs::read_dir("file:/root/etc").is_ok()
    }

    /// Read and parse the administrator's static file.  A generated file or a
    /// missing file is not an override.
    fn read_static_override(path: &str) -> Option<Config> {
        let text = match read_text(path) {
            Ok(text) => text,
            Err(Errno::ENOENT) => return None,
            Err(error) => {
                raven_rt::eprintln!("resolvd: read {path}: {}", error.description());
                return None;
            }
        };
        if !config::has_static_override(&text) {
            return None;
        }
        Some(config::parse(&text))
    }

    fn read_text(path: &str) -> Result<String, Errno> {
        let data = raven_rt::fs::read_to_vec(path)?;
        if data.len() > MAX_CONF_BYTES {
            return Err(Errno::E2BIG);
        }
        core::str::from_utf8(&data)
            .map(str::to_string)
            .map_err(|_| Errno::EINVAL)
    }

    fn write_text(path: &str, text: &str) -> Result<(), Errno> {
        let mut file = raven_rt::fs::open(path, O_WRONLY | O_CREAT | O_TRUNC)?;
        let mut offset = 0usize;
        while offset < text.len() {
            let written = file.write(&text.as_bytes()[offset..])?;
            if written == 0 {
                return Err(Errno::EIO);
            }
            offset += written;
        }
        file.close()
    }

    fn set_resolver(config: Config) {
        if config.servers.is_empty() {
            *RESOLVER.lock() = None;
        } else {
            *RESOLVER.lock() = Some(Resolver::new(config.servers, TIMEOUT_MS, ATTEMPTS));
        }
    }

    fn apply_lease(lease: &Lease, iface: &str, conf: &str) {
        let config = lease.config();
        if lease.has_dns() {
            match write_text(conf, &config.render()) {
                Ok(()) => raven_rt::println!("resolvd: wrote {conf}"),
                Err(error) => {
                    raven_rt::eprintln!("resolvd: write {conf}: {}", error.description());
                }
            }
        }
        set_resolver(config.clone());
        *LAST_LEASE.lock() = Some(lease.clone());
        raven_rt::println!(
            "resolvd: lease {iface} {}.{}.{}.{}/{} router={} dns={} renew={} rebind={} expires={}",
            lease.addr[0],
            lease.addr[1],
            lease.addr[2],
            lease.addr[3],
            lease.prefix,
            lease
                .router
                .map(|router| format!("{}.{}.{}.{}", router[0], router[1], router[2], router[3]))
                .unwrap_or_else(|| "none".to_string()),
            servers_text(&config.servers),
            timer_text(lease.renew_ms),
            timer_text(lease.rebind_ms),
            timer_text(lease.expires_ms),
        );
        if lease.has_dns() {
            raven_rt::println!("resolvd: resolv.conf={}", nameserver_lines(&config.servers));
        }
    }

    fn timer_text(value: u32) -> String {
        if value == DHCP_TIMER_UNKNOWN {
            "unknown".to_string()
        } else {
            format!("{value}ms")
        }
    }

    fn servers_text(servers: &[[u8; 4]]) -> String {
        servers
            .iter()
            .map(|server| format!("{}.{}.{}.{}", server[0], server[1], server[2], server[3]))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn nameserver_lines(servers: &[[u8; 4]]) -> String {
        servers
            .iter()
            .map(|server| {
                format!(
                    "nameserver {}.{}.{}.{}",
                    server[0], server[1], server[2], server[3]
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    fn wait_for_lease(iface: &str, timeout_ms: u64) -> Option<Lease> {
        let mut waited = 0u64;
        loop {
            match query_lease(iface) {
                Ok(lease) => return Some(lease),
                Err(error) => {
                    if waited >= timeout_ms {
                        raven_rt::println!("resolvd: lease {iface}: {}", error.description());
                        return None;
                    }
                }
            }
            let _ = raven_rt::nanosleep(LEASE_RETRY_MS * 1_000_000);
            waited = waited.saturating_add(LEASE_RETRY_MS);
        }
    }

    fn query_lease(iface: &str) -> Result<Lease, Errno> {
        let client = Client::new();
        let handle = client.open("net:", READ_WRITE)?;
        let name = IfName::new(iface).ok_or(Errno::EINVAL)?;
        let request = IfaceRequest::DhcpLease { name };
        let mut encoded = [0u8; 64];
        let length = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
        let mut response = [0u8; 256];
        let result = client.call(handle, &encoded[..length], &mut response);
        let _ = client.close(handle);
        let read = result?;
        match IfaceResponse::decode(&response[..read]).map_err(|_| Errno::EPROTO)? {
            IfaceResponse::DhcpLease(info) => Ok(Lease::from(info)),
            IfaceResponse::Err(error) => Err(error),
            _ => Err(Errno::EPROTO),
        }
    }

    fn lease_worker(_arg: usize) {
        let plan = { LEASE_PLAN.lock().clone() };
        let Some(plan) = plan else {
            return;
        };
        let mut backoff = LEASE_RETRY_MS;
        let mut last_error: Option<Errno> = None;
        loop {
            match query_lease(&plan.iface) {
                Ok(lease) => {
                    backoff = LEASE_RETRY_MS;
                    last_error = None;
                    let changed = {
                        let last = LAST_LEASE.lock();
                        last.as_ref()
                            .is_none_or(|previous| !previous.same_config(&lease))
                    };
                    if changed {
                        apply_lease(&lease, &plan.iface, &plan.conf);
                    } else {
                        *LAST_LEASE.lock() = Some(lease.clone());
                    }
                    let delay = u64::from(lease.refresh_delay_ms());
                    let _ = raven_rt::nanosleep(delay.saturating_mul(1_000_000));
                }
                Err(error) => {
                    if last_error != Some(error) {
                        raven_rt::println!(
                            "resolvd: lease {}: {}",
                            plan.iface,
                            error.description()
                        );
                        last_error = Some(error);
                    }
                    let _ = raven_rt::nanosleep(backoff.saturating_mul(1_000_000));
                    backoff = backoff.saturating_mul(2).min(LEASE_BACKOFF_MAX_MS);
                }
            }
        }
    }

    struct HandlerState {
        opens: OpenTable<()>,
        pending: Option<(u64, String)>,
    }

    impl HandlerState {
        fn new() -> Self {
            Self {
                opens: OpenTable::new(),
                pending: None,
            }
        }

        fn expect_open(&mut self, file_id: u64, path: &str) {
            self.pending = Some((file_id, path.to_string()));
        }
    }

    impl Handler for HandlerState {
        fn open_with(
            &mut self,
            path: &str,
            _flags: raven_proto_fs::OpenFlags,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path || (!path.is_empty() && path != "/") {
                return Err(Errno::ENOENT.into());
            }
            self.opens
                .insert_with_id(file_id, (), raven_proto_fs::OpenFlags::READ)?;
            Ok(Opened::File(file_id))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.opens.close(file)?;
            Ok(())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            Ok(self.opens.dup(file)?)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            if self.opens.get(file).is_none() {
                return Err(Errno::EBADF.into());
            }
            let query = Query::decode(request).map_err(|_| Errno::EINVAL)?;
            let qtype = u16::from(query.qtype);
            let mut guard = RESOLVER.lock();
            let Some(resolver) = guard.as_mut() else {
                return Ok(Reply::error(query.qtype)
                    .encode(response)
                    .map_err(|_| Errno::EPROTO)?);
            };
            raven_rt::println!(
                "resolvd: query {} {} via {}",
                query.name,
                type_name(query.qtype),
                servers_text(resolver.servers())
            );
            let mut exchange = NetExchange::new();
            let reply = match resolver.resolve(&mut exchange, query.name, qtype) {
                Ok(answers) => {
                    let mut records = Vec::new();
                    for (ttl, address) in answers {
                        let address = match address {
                            DnsAddress::V4(bytes) => Some(Address::V4(bytes)),
                            DnsAddress::V6(bytes) if query.qtype == QTYPE_AAAA => {
                                Some(Address::V6(bytes))
                            }
                            _ => None,
                        };
                        if let Some(address) = address {
                            records.push(Record { ttl, address });
                        }
                    }
                    if records.is_empty() {
                        Reply::error(query.qtype)
                    } else {
                        Reply::answers(query.qtype, records)
                    }
                }
                Err(ResolveError::NotFound) => Reply::nxdomain(query.qtype),
                Err(error) => {
                    raven_rt::println!("resolvd: query failed: {:?}", error);
                    Reply::error(query.qtype)
                }
            };
            Ok(reply.encode(response).map_err(|_| Errno::EPROTO)?)
        }
    }

    fn type_name(qtype: u8) -> &'static str {
        match qtype {
            resolvd::proto::QTYPE_A => "A",
            QTYPE_AAAA => "AAAA",
            _ => "?",
        }
    }

    struct NetExchange {
        client: Client,
    }

    impl NetExchange {
        fn new() -> Self {
            Self {
                client: Client::new(),
            }
        }

        fn control(
            &self,
            handle: raven_abi::types::Handle,
            request: &SocketRequest<'_>,
            response: &mut [u8],
        ) -> Result<Response, Errno> {
            let mut encoded = [0u8; 1024];
            let length = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
            let length = self.client.call(handle, &encoded[..length], response)?;
            Response::decode(&response[..length]).map_err(|_| Errno::EPROTO)
        }
    }

    impl Exchange for NetExchange {
        fn query(
            &mut self,
            server: [u8; 4],
            request: &[u8],
            response: &mut [u8],
            timeout_ms: u32,
        ) -> Result<usize, Errno> {
            let path = format!(
                "net:udp/{}.{}.{}.{}:{DNS_PORT}",
                server[0], server[1], server[2], server[3]
            );
            let handle = self.client.open(&path, READ_WRITE)?;
            let result = (|| {
                let mut scratch = [0u8; 64];
                let bound = self.control(
                    handle,
                    &SocketRequest::Call(Call::Bind(Endpoint {
                        ip: [0; 4],
                        port: LOCAL_PORT,
                    })),
                    &mut scratch,
                )?;
                if bound != Response::Ok {
                    return Err(Errno::EINVAL);
                }
                let wrote = self.control(
                    handle,
                    &SocketRequest::Write {
                        data: request,
                        to: Some(Endpoint {
                            ip: server,
                            port: DNS_PORT,
                        }),
                    },
                    &mut scratch,
                )?;
                if !matches!(wrote, Response::Write { len } if len as usize == request.len()) {
                    return Err(Errno::EIO);
                }
                for _ in 0..timeout_ms {
                    match self.client.read(handle, 0, response) {
                        Ok(length) => return Ok(length),
                        Err(Errno::EAGAIN) => {
                            let _ = raven_rt::nanosleep(1_000_000);
                        }
                        Err(error) => return Err(error),
                    }
                }
                Err(Errno::ETIMEDOUT)
            })();
            let _ = self.client.close(handle);
            result
        }
    }

    fn log_started(name: &str) {
        let client = Client::new();
        let path = format!("log:{name}");
        if let Ok(handle) = client.open(&path, 2) {
            let _ = client.write(handle, 0, b"started\n");
            let _ = client.close(handle);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
