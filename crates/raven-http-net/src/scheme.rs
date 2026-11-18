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

use crate::transport::HttpTransport;
use crate::{ResolveError, Resolver, Socket, SocketError};

const READ_WRITE: u32 = 3;
const READ: u32 = 1;
const POLL_MS: u32 = 5;
const RESOLV_VERSION: u16 = 1;
const QTYPE_A: u8 = 1;
const STATUS_OK: u8 = 0;
const MAX_NAME: usize = 253;
const MAX_RECORDS: usize = 8;
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
    fn resolve(&mut self, host: &str, _timeout_ms: u32) -> Result<Ipv4Addr, ResolveError> {
        if let Ok(address) = host.parse::<Ipv4Addr>() {
            return Ok(address);
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
    let _ = raven_rt::nanosleep(u64::from(POLL_MS) * 1_000_000);
    *remaining = remaining.saturating_sub(POLL_MS);
    true
}

fn encode_query(name: &str, out: &mut [u8]) -> Result<usize, ResolveError> {
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(ResolveError::NotFound);
    }
    for label in name.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(ResolveError::NotFound);
        }
    }
    let length = u16::try_from(name.len()).map_err(|_| ResolveError::NotFound)?;
    let total = 5 + name.len();
    let slot = out.get_mut(..total).ok_or(ResolveError::Io)?;
    slot[..2].copy_from_slice(&RESOLV_VERSION.to_le_bytes());
    slot[2] = QTYPE_A;
    slot[3..5].copy_from_slice(&length.to_le_bytes());
    slot[5..total].copy_from_slice(name.as_bytes());
    Ok(total)
}

fn decode_reply(input: &[u8]) -> Result<Ipv4Addr, ResolveError> {
    if input.len() < 5
        || u16::from_le_bytes([input[0], input[1]]) != RESOLV_VERSION
        || input[3] != QTYPE_A
    {
        return Err(ResolveError::Io);
    }
    if input[2] != STATUS_OK {
        return Err(ResolveError::NotFound);
    }
    let count = usize::from(input[4]);
    let total = 5 + count.saturating_mul(9);
    if count == 0 || count > MAX_RECORDS || input.len() != total {
        return Err(ResolveError::NotFound);
    }
    let record = input.get(5..14).ok_or(ResolveError::Io)?;
    if record[0] != QTYPE_A {
        return Err(ResolveError::Io);
    }
    Ok(Ipv4Addr::new(record[5], record[6], record[7], record[8]))
}
