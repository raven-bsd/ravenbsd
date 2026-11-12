// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD DNS resolver service (`resolv:` scheme).
//!
//! The target reads nameservers from the initial `/etc/resolv.conf`, performs
//! bounded A/AAAA queries through `netd`'s UDP sockets, and returns the answer
//! or NXDOMAIN to a caller.  DHCP lease application is pending the B12b/B12c
//! netd seam.

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
    use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
    use raven_rt::entry;
    use raven_scheme::{Client, Ctx, Handler, HandlerResult, OpenTable, Opened, Server};
    use resolvd::config;
    use resolvd::dns::Address as DnsAddress;
    use resolvd::proto::{Address, QTYPE_AAAA, Query, Record, Reply};
    use resolvd::resolver::{Exchange, ResolveError, Resolver};

    entry!(main);

    const OPEN_OPCODE: u16 = 1;
    const MAX_CONF_BYTES: usize = 4096;
    const DEFAULT_CONF: &str = "initfs:/etc/resolv.conf";
    const DNS_PORT: u16 = 53;
    const LOCAL_PORT: u16 = 5301;
    const TIMEOUT_MS: u32 = 500;
    const ATTEMPTS: u8 = 2;

    fn main(_args: raven_rt::Args) -> i32 {
        let servers = match load_servers() {
            Ok(servers) => servers,
            Err(message) => {
                raven_rt::eprintln!("resolvd: {message}");
                return 1;
            }
        };
        if servers.is_empty() {
            raven_rt::eprintln!("resolvd: no nameservers in {DEFAULT_CONF}");
            return 1;
        }
        raven_rt::println!("resolvd: nameservers={}", servers.len());
        let mut handler = HandlerState::new(Resolver::new(servers, TIMEOUT_MS, ATTEMPTS));
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

    fn load_servers() -> Result<Vec<[u8; 4]>, String> {
        let client = Client::new();
        let handle = client
            .open(DEFAULT_CONF, 1)
            .map_err(|error| format!("open {DEFAULT_CONF}: {}", error.description()))?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("read {DEFAULT_CONF}: {}", error.description()))?;
            if read == 0 {
                break;
            }
            if data.len() + read > MAX_CONF_BYTES {
                let _ = client.close(handle);
                return Err(format!("{DEFAULT_CONF} exceeds {MAX_CONF_BYTES} bytes"));
            }
            data.extend_from_slice(&buffer[..read]);
        }
        let _ = client.close(handle);
        let text =
            core::str::from_utf8(&data).map_err(|_| format!("{DEFAULT_CONF} is not UTF-8"))?;
        Ok(config::parse(text).servers)
    }

    struct HandlerState {
        resolver: Resolver,
        opens: OpenTable<()>,
        pending: Option<(u64, String)>,
    }

    impl HandlerState {
        fn new(resolver: Resolver) -> Self {
            Self {
                resolver,
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
            let mut exchange = NetExchange::new();
            let reply =
                match self
                    .resolver
                    .resolve(&mut exchange, query.name, u16::from(query.qtype))
                {
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
            let handle = self.client.open(&path, 3)?;
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
