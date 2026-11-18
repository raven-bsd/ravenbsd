// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The request/response state machine and redirect policy.

use crate::request::Request;
use crate::response::{self, Reader, Response};
use crate::url::Scheme;
use crate::{Clock, Error, Transport, TransportError};

/// One HTTP GET exchange over a caller-owned transport and clock.
pub struct Client<'a, T: Transport, C: Clock> {
    transport: &'a mut T,
    clock: &'a C,
}

impl<'a, T: Transport, C: Clock> Client<'a, T, C> {
    /// Create a client over `transport` and `clock`.
    pub fn new(transport: &'a mut T, clock: &'a C) -> Self {
        Self { transport, clock }
    }

    /// Perform a GET, following up to the request's redirect budget.
    ///
    /// A total timeout covers connect, write, response headers and body. A redirect to
    /// `https://` returns [`Error::UnsupportedScheme`] so the caller can hand the URL to the
    /// owner-approved TLS add-on instead of silently downgrading it.
    pub fn get(&mut self, request: &Request) -> Result<Response, Error> {
        let mut url = request.url().clone();
        if url.scheme != Scheme::Http {
            return Err(Error::UnsupportedScheme);
        }
        let mut same_origin = true;
        let max_redirects = request.max_redirects();
        for attempt in 0..=max_redirects {
            let deadline = self
                .clock
                .now_ms()
                .saturating_add(u64::from(request.timeout_ms()));
            let timeout_ms = remaining(self.clock, deadline)?;
            self.transport
                .connect(&url.host, url.port, timeout_ms)
                .map_err(map_transport)?;
            let wire = request.encode(&url, same_origin);
            write_all(self.transport, self.clock, deadline, &wire)?;
            let mut reader = Reader::new(&mut *self.transport, self.clock, deadline);
            let response = response::parse(&mut reader, request.max_body())?;
            if !response.is_redirect() {
                return Ok(response);
            }
            if attempt == max_redirects {
                return Err(Error::TooManyRedirects);
            }
            let location = response.location().ok_or(Error::MissingLocation)?;
            let next = url.resolve(location)?;
            if next.scheme != Scheme::Http {
                return Err(Error::UnsupportedScheme);
            }
            same_origin = url.host == next.host && url.port == next.port;
            url = next;
        }
        Err(Error::TooManyRedirects)
    }
}

fn map_transport(error: TransportError) -> Error {
    match error {
        TransportError::Timeout => Error::Timeout,
        TransportError::Io => Error::Transport,
    }
}

fn write_all<T: Transport, C: Clock>(
    transport: &mut T,
    clock: &C,
    deadline: u64,
    bytes: &[u8],
) -> Result<(), Error> {
    let mut sent = 0usize;
    while sent < bytes.len() {
        let timeout_ms = remaining(clock, deadline)?;
        match transport.write(&bytes[sent..], timeout_ms) {
            Ok(0) => return Err(Error::Transport),
            Ok(count) if count <= bytes.len() - sent => sent += count,
            Ok(_) => return Err(Error::Transport),
            Err(error) => return Err(map_transport(error)),
        }
    }
    Ok(())
}

fn remaining<C: Clock>(clock: &C, deadline: u64) -> Result<u32, Error> {
    let now = clock.now_ms();
    if now >= deadline {
        return Err(Error::Timeout);
    }
    Ok(u32::try_from(deadline - now).unwrap_or(u32::MAX))
}

#[cfg(test)]
#[path = "client_ut.rs"]
mod tests;
