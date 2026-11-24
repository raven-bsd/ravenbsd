// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The GET request builder and wire encoder.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::header::Header;
use crate::url::{Scheme, Url};
use crate::{DEFAULT_MAX_BODY, DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT_MS, Error, MAX_URL};

/// One plain-HTTP GET request and its policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    url: Url,
    headers: Vec<Header>,
    user_agent: String,
    timeout_ms: u32,
    max_redirects: u8,
    max_body: usize,
}

impl Request {
    /// The initial URL.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Extra request headers.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// Total timeout in milliseconds.
    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    /// Maximum number of redirects to follow.
    pub fn max_redirects(&self) -> u8 {
        self.max_redirects
    }

    /// Maximum body bytes accepted.
    pub fn max_body(&self) -> usize {
        self.max_body
    }

    /// Encode the request for `url`.
    ///
    /// `same_origin` is false on a cross-origin redirect; sensitive credentials are then
    /// omitted instead of leaking to the new host.
    pub(crate) fn encode(&self, url: &Url, same_origin: bool) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"GET ");
        out.extend_from_slice(url.path.as_bytes());
        out.extend_from_slice(b" HTTP/1.1\r\n");
        push_header(&mut out, "Host", &url.host_header());
        push_header(&mut out, "User-Agent", &self.user_agent);
        push_header(&mut out, "Accept", "*/*");
        push_header(&mut out, "Accept-Encoding", "identity");
        push_header(&mut out, "Connection", "close");
        for header in &self.headers {
            if !same_origin && sensitive(&header.name) {
                continue;
            }
            push_header(&mut out, &header.name, &header.value);
        }
        out.extend_from_slice(b"\r\n");
        out
    }
}

/// Build a [`Request`] with bounded defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestBuilder {
    url: Url,
    headers: Vec<Header>,
    user_agent: String,
    timeout_ms: u32,
    max_redirects: u8,
    max_body: usize,
}

impl RequestBuilder {
    /// Start a GET request for an absolute `http://` URL.
    ///
    /// `https://` is rejected here: the core is the plain-HTTP path and the E3 decision routes
    /// HTTPS through the OpenSSL add-on process.
    pub fn new(url: &str) -> Result<Self, Error> {
        if url.len() > MAX_URL {
            return Err(Error::InvalidUrl);
        }
        let url = Url::parse(url)?;
        if url.scheme != Scheme::Http {
            return Err(Error::UnsupportedScheme);
        }
        Ok(Self {
            url,
            headers: Vec::new(),
            user_agent: String::from("raven-http/0.1"),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            max_body: DEFAULT_MAX_BODY,
        })
    }

    /// Add one request header.
    ///
    /// `Host`, `Connection`, `Content-Length`, `Transfer-Encoding` and `User-Agent` are reserved
    /// and rejected; the encoder owns them.
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, Error> {
        if reserved(name) {
            return Err(Error::InvalidRequest);
        }
        let header = Header::new(name, value).ok_or(Error::InvalidRequest)?;
        self.headers.push(header);
        Ok(self)
    }

    /// Replace the default `User-Agent`.
    pub fn user_agent(mut self, value: &str) -> Result<Self, Error> {
        if value.is_empty() || !crate::header::valid_value(value) {
            return Err(Error::InvalidRequest);
        }
        self.user_agent = value.to_string();
        Ok(self)
    }

    /// Set the total timeout in milliseconds. Zero means an immediate timeout.
    pub const fn timeout_ms(mut self, value: u32) -> Self {
        self.timeout_ms = value;
        self
    }

    /// Set the redirect budget.
    pub const fn max_redirects(mut self, value: u8) -> Self {
        self.max_redirects = value;
        self
    }

    /// Set the maximum body bytes accepted.
    pub const fn max_body(mut self, value: usize) -> Self {
        self.max_body = value;
        self
    }

    /// Finish the request.
    pub fn build(self) -> Request {
        Request {
            url: self.url,
            headers: self.headers,
            user_agent: self.user_agent,
            timeout_ms: self.timeout_ms,
            max_redirects: self.max_redirects,
            max_body: self.max_body,
        }
    }
}

fn push_header(out: &mut Vec<u8>, name: &str, value: &str) {
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(value.as_bytes());
    out.extend_from_slice(b"\r\n");
}

fn reserved(name: &str) -> bool {
    [
        "host",
        "connection",
        "content-length",
        "transfer-encoding",
        "user-agent",
    ]
    .iter()
    .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

fn sensitive(name: &str) -> bool {
    ["authorization", "cookie", "proxy-authorization"]
        .iter()
        .any(|sensitive| name.eq_ignore_ascii_case(sensitive))
}

#[cfg(test)]
#[path = "request_ut.rs"]
mod tests;
