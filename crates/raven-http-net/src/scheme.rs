// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target implementation over the `net:` and `resolv:` schemes.
//!
//! `netd` owns the TCP/IP stack and serves `net:tcp/<ipv4>:<port>`; `resolvd` owns DNS and serves
//! `resolv:`. The adapter opens one scheme handle per operation, maps `EAGAIN` into a bounded
//! sleep/retry loop and never touches a device or a C socket. This is the platform socket API
//! for 0.1.0; the raw libc lazy-socket path is not complete.

use alloc::format;
use core::net::Ipv4Addr;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
use raven_scheme::Client;

use crate::resolv::{MAX_NAME, decode_reply, encode_query};
use crate::transport::HttpTransport;
use crate::{ResolveError, Resolved, Resolver, Socket, SocketError};

const READ_WRITE: u32 = 3;
const READ: u32 = 1;
const POLL_MS: u32 = 5;
const REPLY_LIMIT: usize = 256;

/// The live target transport: `net:` sockets plus `resolv:` DNS.
pub fn live_transport() -> HttpTransport<SchemeSocket, SchemeResolver> {
    HttpTransport::new(SchemeSocket::new(), SchemeResolver::new())
}

/// A TCP socket served by `netd`.
pub struct SchemeSocket {
    client: Client,
    handle: Option<Handle>,
}

impl SchemeSocket {
    /// Create a socket with no open `net:` handle.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            handle: None,
        }
    }

    fn close_handle(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = self.client.close(handle);
        }
    }

    fn call(&self, handle: Handle, operation: Call) -> Result<(), Errno> {
        let request = SocketRequest::Call(operation);
        let mut encoded = [0u8; 64];
        let length = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
        let mut response = [0u8; 128];
        let read = self
            .client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|_| Errno::EIO)?;
        match Response::decode(&response[..read]).map_err(|_| Errno::EINVAL)? {
            Response::Ok => Ok(()),
            Response::Err(error) => Err(error),
            _ => Err(Errno::EINVAL),
        }
    }
}

impl Default for SchemeSocket {
    fn default() -> Self {
        Self::new()
    }
}

impl Socket for SchemeSocket {
    fn connect(
        &mut self,
        address: Ipv4Addr,
        port: u16,
        timeout_ms: u32,
    ) -> Result<(), SocketError> {
        self.close_handle();
        let path = format!("net:tcp/{address}:{port}");
        let handle = self
            .client
            .open(&path, READ_WRITE)
            .map_err(|_| SocketError::Io)?;
        self.handle = Some(handle);
        let endpoint = Endpoint {
            ip: address.octets(),
            port,
        };
        let mut remaining = timeout_ms;
        loop {
            match self.call(handle, Call::Connect(endpoint)) {
                Ok(()) => return Ok(()),
                Err(Errno::EAGAIN) => {
                    if !sleep_slice(&mut remaining) {
                        self.close_handle();
                        return Err(SocketError::Timeout);
                    }
                }
                Err(_) => {
                    self.close_handle();
                    return Err(SocketError::Io);
                }
            }
        }
    }

    fn send(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<usize, SocketError> {
        let handle = self.handle.ok_or(SocketError::Io)?;
        let mut remaining = timeout_ms;
        loop {
            match self.client.write(handle, 0, bytes) {
                Ok(count) => return Ok(count),
                Err(Errno::EAGAIN) => {
                    if !sleep_slice(&mut remaining) {
                        return Err(SocketError::Timeout);
                    }
                }
                Err(_) => return Err(SocketError::Io),
            }
        }
    }

    fn recv(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, SocketError> {
        let handle = self.handle.ok_or(SocketError::Io)?;
        let mut remaining = timeout_ms;
        loop {
            match self.client.read(handle, 0, bytes) {
                Ok(0) => return Ok(0),
                Ok(count) => return Ok(count),
                Err(Errno::EAGAIN) => {
                    if !sleep_slice(&mut remaining) {
                        return Err(SocketError::Timeout);
                    }
                }
                Err(_) => return Err(SocketError::Io),
            }
        }
    }

    fn close(&mut self) -> Result<(), SocketError> {
        self.close_handle();
        Ok(())
    }
}

/// A DNS resolver that speaks the `resolv:` scheme.
pub struct SchemeResolver {
    client: Client,
}

impl SchemeResolver {
    /// Create a resolver with a scheme client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for SchemeResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver for SchemeResolver {
    fn resolve(&mut self, host: &str, _timeout_ms: u32) -> Result<Resolved, ResolveError> {
        if let Ok(address) = host.parse::<Ipv4Addr>() {
            return Ok(Resolved {
                address,
                ttl_secs: 0,
            });
        }
        let mut query = [0u8; MAX_NAME + 5];
        let query_len = encode_query(host, &mut query)?;
        let handle = self
            .client
            .open("resolv:", READ)
            .map_err(|_| ResolveError::Io)?;
        let mut reply = [0u8; REPLY_LIMIT];
        let result = self.client.call(handle, &query[..query_len], &mut reply);
        let _ = self.client.close(handle);
        let read = result.map_err(|error| {
            if error == Errno::ETIMEDOUT {
                ResolveError::Timeout
            } else {
                ResolveError::Io
            }
        })?;
        decode_reply(&reply[..read])
    }
}

fn sleep_slice(remaining: &mut u32) -> bool {
    if *remaining == 0 {
        return false;
    }
    let slice = (*remaining).min(POLL_MS);
    let _ = raven_rt::nanosleep(u64::from(slice) * 1_000_000);
    *remaining -= slice;
    true
}
