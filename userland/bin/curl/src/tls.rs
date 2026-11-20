// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target transport for the `curl`/`wget` TLS add-on build.
//!
//! The HTTP core still sees a byte stream. This module resolves the host in `resolvd`, opens a
//! `net:` TCP socket, optionally wraps it in a certificate- and hostname-verified `raven-tls`
//! connection, and exposes the result as a `raven_http::Transport`. It contains no TLS or crypto
//! code; the handshake is upstream OpenSSL through the add-on (DR-45).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::ffi::{c_int, c_long};
use core::net::Ipv4Addr;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY, OpenFlags};
use raven_proto_net::{Call, Endpoint, Response as SocketResponse, SocketRequest};
use raven_scheme::Client;
use raven_tls::{Connection, Error as TlsError, Transport as TlsIo, TransportError as TlsIoError};

use raven_http::{Transport as HttpTransport, TransportError as HttpTransportError};

const READ_WRITE: u32 = 3;
const READ: u32 = 1;
const POLL_MS: u32 = 5;
const REPLY_LIMIT: usize = 256;
const MAX_NAME: usize = 253;
const MAX_RECORDS: usize = 8;

#[repr(C)]
struct Timespec {
    seconds: c_long,
    nanoseconds: c_long,
}

unsafe extern "C" {
    fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> c_int;
}

fn sleep_ms(milliseconds: u32) {
    let request = Timespec {
        seconds: c_long::from(milliseconds / 1_000),
        nanoseconds: c_long::from((milliseconds % 1_000) * 1_000_000),
    };
    // SAFETY: `request` is a valid timespec and a null remaining pointer is accepted by libc.
    unsafe {
        let _ = nanosleep(&request, core::ptr::null_mut());
    }
}

fn sleep_slice(remaining: &mut u32) -> bool {
    if *remaining == 0 {
        return false;
    }
    let slice = (*remaining).min(POLL_MS);
    sleep_ms(slice);
    *remaining -= slice;
    true
}

struct SchemeSocket {
    client: Client,
    handle: Option<Handle>,
}

impl SchemeSocket {
    fn new() -> Self {
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

    fn connect(&mut self, address: Ipv4Addr, port: u16, timeout_ms: u32) -> Result<(), ()> {
        self.close_handle();
        let path = format!("net:tcp/{address}:{port}");
        let handle = self.client.open(&path, READ_WRITE).map_err(|_| ())?;
        self.handle = Some(handle);
        let request = SocketRequest::Call(Call::Connect(Endpoint {
            ip: address.octets(),
            port,
        }));
        let mut encoded = [0u8; 64];
        let length = request.encode(&mut encoded).map_err(|_| ())?;
        let mut response = [0u8; 64];
        let mut remaining = timeout_ms;
        loop {
            match self.client.call(handle, &encoded[..length], &mut response) {
                Ok(read) => match SocketResponse::decode(&response[..read]) {
                    Ok(SocketResponse::Ok) => return Ok(()),
                    Ok(SocketResponse::Err(error)) => {
                        if error == Errno::EAGAIN {
                            if !sleep_slice(&mut remaining) {
                                self.close_handle();
                                return Err(());
                            }
                        } else {
                            self.close_handle();
                            return Err(());
                        }
                    }
                    Ok(_) | Err(_) => {
                        self.close_handle();
                        return Err(());
                    }
                },
                Err(Errno::EAGAIN) => {
                    if !sleep_slice(&mut remaining) {
                        self.close_handle();
                        return Err(());
                    }
                }
                Err(_) => {
                    self.close_handle();
                    return Err(());
                }
            }
        }
    }

    fn send(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<usize, ()> {
        let handle = self.handle.ok_or(())?;
        let mut remaining = timeout_ms;
        loop {
            match self.client.write(handle, 0, bytes) {
                Ok(count) if count > 0 => return Ok(count),
                Ok(_) | Err(Errno::EAGAIN) => {
                    if !sleep_slice(&mut remaining) {
                        return Err(());
                    }
                }
                Err(_) => return Err(()),
            }
        }
    }

    fn recv(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, ()> {
        let handle = self.handle.ok_or(())?;
        let mut remaining = timeout_ms;
        loop {
            match self.client.read(handle, 0, bytes) {
                Ok(count) => return Ok(count),
                Err(Errno::EAGAIN) => {
                    if !sleep_slice(&mut remaining) {
                        return Err(());
                    }
                }
                Err(_) => return Err(()),
            }
        }
    }

    fn close(&mut self) {
        self.close_handle();
    }
}

impl Drop for SchemeSocket {
    fn drop(&mut self) {
        self.close_handle();
    }
}

struct SchemeResolver {
    client: Client,
}

impl SchemeResolver {
    fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn resolve(&self, host: &str) -> Result<Ipv4Addr, ()> {
        if let Ok(address) = host.parse::<Ipv4Addr>() {
            return Ok(address);
        }
        let mut query = [0u8; MAX_NAME + 5];
        let length = encode_query(host, &mut query)?;
        let handle = self.client.open("resolv:", READ).map_err(|_| ())?;
        let mut reply = [0u8; REPLY_LIMIT];
        let result = self.client.call(handle, &query[..length], &mut reply);
        let _ = self.client.close(handle);
        let read = result.map_err(|_| ())?;
        decode_reply(&reply[..read]).map(|(address, _ttl)| address)
    }
}

fn encode_query(name: &str, out: &mut [u8]) -> Result<usize, ()> {
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(());
    }
    for label in name.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(());
        }
    }
    let length = u16::try_from(name.len()).map_err(|_| ())?;
    let total = 5 + name.len();
    let slot = out.get_mut(..total).ok_or(())?;
    slot[..2].copy_from_slice(&1u16.to_le_bytes());
    slot[2] = 1;
    slot[3..5].copy_from_slice(&length.to_le_bytes());
    slot[5..total].copy_from_slice(name.as_bytes());
    Ok(total)
}

fn decode_reply(input: &[u8]) -> Result<(Ipv4Addr, u32), ()> {
    if input.len() < 5 || u16::from_le_bytes([input[0], input[1]]) != 1 || input[3] != 1 {
        return Err(());
    }
    if input[2] != 0 {
        return Err(());
    }
    let count = usize::from(input[4]);
    let total = 5 + count.saturating_mul(9);
    if count == 0 || count > MAX_RECORDS || input.len() != total {
        return Err(());
    }
    let record = input.get(5..14).ok_or(())?;
    if record[0] != 1 {
        return Err(());
    }
    let ttl = u32::from_le_bytes([record[1], record[2], record[3], record[4]]);
    Ok((
        Ipv4Addr::new(record[5], record[6], record[7], record[8]),
        ttl,
    ))
}

struct TlsStream {
    socket: SchemeSocket,
    timeout_ms: u32,
}

impl TlsIo for TlsStream {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, TlsIoError> {
        self.socket
            .recv(buffer, self.timeout_ms)
            .map_err(|()| TlsIoError)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, TlsIoError> {
        self.socket
            .send(buffer, self.timeout_ms)
            .map_err(|()| TlsIoError)
    }
}

impl Drop for TlsStream {
    fn drop(&mut self) {
        self.socket.close();
    }
}

enum Stream {
    Plain(SchemeSocket),
    Tls(Connection),
}

/// A `raven_http::Transport` that can run plain HTTP or verified HTTPS over the `net:` scheme.
pub struct TlsTransport {
    resolver: SchemeResolver,
    stream: Option<Stream>,
    ca_file: String,
    tls: bool,
    last_error: Option<&'static str>,
}

impl TlsTransport {
    /// Build a transport. `tls` selects the OpenSSL handshake; the same socket path is used for
    /// plain HTTP.
    #[must_use]
    pub fn new(ca_file: &str, tls: bool) -> Self {
        Self {
            resolver: SchemeResolver::new(),
            stream: None,
            ca_file: String::from(ca_file),
            tls,
            last_error: None,
        }
    }

    /// The last TLS failure, if the previous `connect` failed in the handshake or verification.
    #[must_use]
    pub fn last_error(&self) -> Option<&'static str> {
        self.last_error
    }

    fn close_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            match stream {
                Stream::Plain(mut socket) => socket.close(),
                Stream::Tls(connection) => drop(connection),
            }
        }
    }
}

impl HttpTransport for TlsTransport {
    fn connect(
        &mut self,
        host: &str,
        port: u16,
        timeout_ms: u32,
    ) -> Result<(), HttpTransportError> {
        self.close_stream();
        self.last_error = None;
        let address = self
            .resolver
            .resolve(host)
            .map_err(|()| HttpTransportError::Io)?;
        let mut socket = SchemeSocket::new();
        let mut attempt = 0u32;
        loop {
            match socket.connect(address, port, timeout_ms) {
                Ok(()) => break,
                Err(()) if attempt < 9 => {
                    attempt = attempt.saturating_add(1);
                    // The netd TCP readiness/retransmission race can reset the first connect;
                    // retry the connect only, never a TLS handshake or verification failure.
                    sleep_ms(100);
                    socket = SchemeSocket::new();
                }
                Err(()) => return Err(HttpTransportError::Io),
            }
        }
        if self.tls {
            let stream = TlsStream { socket, timeout_ms };
            match raven_tls::connect(Box::new(stream), host, &self.ca_file) {
                Ok(connection) => self.stream = Some(Stream::Tls(connection)),
                Err(error) => {
                    self.last_error = Some(tls_error_message(error));
                    return Err(HttpTransportError::Io);
                }
            }
        } else {
            self.stream = Some(Stream::Plain(socket));
        }
        Ok(())
    }

    fn write(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<usize, HttpTransportError> {
        match self.stream.as_mut() {
            Some(Stream::Plain(socket)) => socket
                .send(bytes, timeout_ms)
                .map_err(|()| HttpTransportError::Io),
            Some(Stream::Tls(connection)) => {
                connection.write(bytes).map_err(|_| HttpTransportError::Io)
            }
            None => Err(HttpTransportError::Io),
        }
    }

    fn read(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, HttpTransportError> {
        match self.stream.as_mut() {
            Some(Stream::Plain(socket)) => socket
                .recv(bytes, timeout_ms)
                .map_err(|()| HttpTransportError::Io),
            Some(Stream::Tls(connection)) => {
                connection.read(bytes).map_err(|_| HttpTransportError::Io)
            }
            None => Err(HttpTransportError::Io),
        }
    }
}

impl Drop for TlsTransport {
    fn drop(&mut self) {
        self.close_stream();
    }
}

fn tls_error_message(error: TlsError) -> &'static str {
    match error {
        TlsError::HostTooLong => "host name too long",
        TlsError::CaPathTooLong => "CA path too long",
        TlsError::Context => "OpenSSL context failed",
        TlsError::CaBundle => "CA bundle missing or malformed",
        TlsError::Bio => "OpenSSL BIO failed",
        TlsError::Handshake => "TLS handshake failed",
        TlsError::Verify(_) => "certificate verification failed",
        TlsError::Io => "TLS I/O failed",
    }
}

/// Write a downloaded body to an absolute or working-directory-relative file.
///
/// The add-on build uses the scheme client directly because the `raven-rt` allocator/entry are
/// not linked into the libc/OpenSSL binary.
///
/// # Errors
///
/// Returns the scheme error when the file cannot be opened, written or closed.
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), Errno> {
    let absolute = if path.starts_with('/') {
        String::from(path)
    } else {
        format!("/{path}")
    };
    let scheme = format!("file:{absolute}");
    let flags = OpenFlags::from_linux(O_WRONLY | O_CREAT | O_TRUNC).map_err(|_| Errno::EINVAL)?;
    let client = Client::new();
    let handle = client.open_flags(&scheme, flags)?;
    let mut offset = 0usize;
    let mut result = Ok(());
    while offset < bytes.len() {
        match client.write(handle, 0, bytes.get(offset..).unwrap_or(&[])) {
            Ok(0) => {
                result = Err(Errno::EIO);
                break;
            }
            Ok(written) => offset = offset.saturating_add(written),
            Err(error) => {
                result = Err(error);
                break;
            }
        }
    }
    let _ = client.close(handle);
    result
}
