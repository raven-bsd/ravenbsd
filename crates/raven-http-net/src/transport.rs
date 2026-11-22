// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The adapter that turns [`Socket`] + [`Resolver`] into a [`raven_http::Transport`].

use core::net::Ipv4Addr;

use raven_http::{Transport, TransportError};

use crate::{ResolveError, Resolver, Socket, SocketError};

/// An HTTP transport over one socket and one resolver.
///
/// A redirect to a different host calls `connect` again: the previous socket is closed and the
/// new host is resolved before the new connection. Dropping the transport closes the socket, so
/// a clean response is not left half-open.
pub struct HttpTransport<S: Socket, R: Resolver> {
    socket: S,
    resolver: R,
    open: bool,
}

impl<S: Socket, R: Resolver> HttpTransport<S, R> {
    /// Build a transport from a socket and a resolver.
    pub const fn new(socket: S, resolver: R) -> Self {
        Self {
            socket,
            resolver,
            open: false,
        }
    }

    /// Borrow the socket (for a caller that needs its identity in diagnostics).
    pub const fn socket(&self) -> &S {
        &self.socket
    }

    /// Borrow the resolver.
    pub const fn resolver(&self) -> &R {
        &self.resolver
    }
}

impl<S: Socket, R: Resolver> Transport for HttpTransport<S, R> {
    fn connect(&mut self, host: &str, port: u16, timeout_ms: u32) -> Result<(), TransportError> {
        if self.open {
            let _ = self.socket.close();
            self.open = false;
        }
        // An IPv4 literal is already an address; do not turn it into a DNS query.
        let address = match host.parse::<Ipv4Addr>() {
            Ok(address) => address,
            Err(_) => {
                self.resolver
                    .resolve(host, timeout_ms)
                    .map_err(map_resolve)?
                    .address
            }
        };
        match self.socket.connect(address, port, timeout_ms) {
            Ok(()) => {
                self.open = true;
                Ok(())
            }
            Err(error) => {
                let _ = self.socket.close();
                Err(map_socket(error))
            }
        }
    }

    fn write(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<usize, TransportError> {
        if !self.open {
            return Err(TransportError::Io);
        }
        self.socket.send(bytes, timeout_ms).map_err(map_socket)
    }

    fn read(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, TransportError> {
        if !self.open {
            return Err(TransportError::Io);
        }
        self.socket.recv(bytes, timeout_ms).map_err(map_socket)
    }
}

impl<S: Socket, R: Resolver> Drop for HttpTransport<S, R> {
    fn drop(&mut self) {
        let _ = self.socket.close();
    }
}

fn map_socket(error: SocketError) -> TransportError {
    match error {
        SocketError::Timeout => TransportError::Timeout,
        SocketError::Io => TransportError::Io,
    }
}

fn map_resolve(error: ResolveError) -> TransportError {
    match error {
        ResolveError::Timeout => TransportError::Timeout,
        ResolveError::NotFound | ResolveError::Io => TransportError::Io,
    }
}

#[cfg(test)]
#[path = "transport_ut.rs"]
mod tests;
