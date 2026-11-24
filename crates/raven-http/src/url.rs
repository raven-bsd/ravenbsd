// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A small absolute-URL parser and reference resolver for plain HTTP.
//!
//! This is not a general URI implementation. It covers what an HTTP GET needs and what a
//! `Location` header can produce: an `http`/`https` scheme, a DNS name or IPv4/IPv6 literal,
//! an optional port, an origin-form path and a query. Userinfo, fragments and non-HTTP schemes
//! are rejected or removed before a request is built.

use alloc::format;
use alloc::string::String;

use crate::{Error, MAX_URL};

/// The two schemes the resolver understands. The client executes only [`Scheme::Http`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// Plain HTTP/1.1.
    Http,
    /// HTTPS; parsed for redirect policy, never opened by this core.
    Https,
}

impl Scheme {
    /// Default port for the scheme.
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }

    /// Lower-case scheme text.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// One absolute HTTP URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    /// Scheme.
    pub scheme: Scheme,
    /// Host name, IPv4 literal, or bracketed IPv6 literal, lower-cased.
    pub host: String,
    /// Effective port (the scheme default when the URL omitted one).
    pub port: u16,
    /// Origin-form path, including the query string, always starting with `/`.
    pub path: String,
}

impl Url {
    /// Parse an absolute `http://` or `https://` URL.
    pub fn parse(input: &str) -> Result<Self, Error> {
        if input.is_empty() || input.len() > MAX_URL {
            return Err(Error::InvalidUrl);
        }
        if input.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
            return Err(Error::InvalidUrl);
        }
        let without_fragment = input.split('#').next().unwrap_or(input);
        let (scheme, rest) = split_scheme(without_fragment)?;
        Self::from_authority_and_path(scheme, rest)
    }

    /// Resolve a `Location` reference against this URL.
    ///
    /// Absolute URLs, network-path references (`//host/x`), absolute paths, query-only
    /// references and relative paths are supported. Fragments are removed.
    pub fn resolve(&self, reference: &str) -> Result<Self, Error> {
        if reference.len() > MAX_URL {
            return Err(Error::InvalidUrl);
        }
        if reference.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
            return Err(Error::InvalidUrl);
        }
        let reference = reference.split('#').next().unwrap_or(reference);
        if reference.is_empty() {
            return Ok(self.clone());
        }
        if reference.starts_with("http://") || reference.starts_with("https://") {
            return Self::parse(reference);
        }
        if let Some(rest) = reference.strip_prefix("//") {
            return Self::from_authority_and_path(self.scheme, rest);
        }

        let (path, query) = split_query(reference);
        let mut resolved = self.clone();
        if path.is_empty() {
            if let Some(query) = query {
                let base = self.path.split('?').next().unwrap_or("/");
                resolved.path = format!("{base}?{query}");
            }
            return Ok(resolved);
        }
        if path.starts_with('/') {
            resolved.path = join_query(remove_dot_segments(path), query);
        } else {
            let base = self.path.split('?').next().unwrap_or("/");
            let directory = match base.rfind('/') {
                Some(index) => &base[..=index],
                None => "/",
            };
            resolved.path = join_query(remove_dot_segments(&format!("{directory}{path}")), query);
        }
        Ok(resolved)
    }

    /// The value for the `Host` header, including a non-default port.
    pub fn host_header(&self) -> String {
        if self.port == self.scheme.default_port() {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn from_authority_and_path(scheme: Scheme, rest: &str) -> Result<Self, Error> {
        let (authority, path) = split_authority(rest);
        let (host, port) = parse_authority(authority, scheme)?;
        let path = if path.is_empty() {
            String::from("/")
        } else if path.starts_with('/') {
            String::from(path)
        } else if let Some(query) = path.strip_prefix('?') {
            format!("/?{query}")
        } else {
            return Err(Error::InvalidUrl);
        };
        Ok(Self {
            scheme,
            host,
            port,
            path,
        })
    }
}

fn split_scheme(input: &str) -> Result<(Scheme, &str), Error> {
    if let Some(rest) = input.strip_prefix("http://") {
        return Ok((Scheme::Http, rest));
    }
    if let Some(rest) = input.strip_prefix("https://") {
        return Ok((Scheme::Https, rest));
    }
    Err(Error::InvalidUrl)
}

fn split_authority(input: &str) -> (&str, &str) {
    match input.find(['/', '?']) {
        Some(index) => (&input[..index], &input[index..]),
        None => (input, ""),
    }
}

fn parse_authority(authority: &str, scheme: Scheme) -> Result<(String, u16), Error> {
    if authority.is_empty() || authority.contains('@') {
        return Err(Error::InvalidUrl);
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err(Error::InvalidUrl);
        };
        let host = &rest[..end];
        if host.is_empty()
            || !host
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b':' || byte == b'.')
        {
            return Err(Error::InvalidUrl);
        }
        let after = &rest[end + 1..];
        let port = parse_port(after, scheme)?;
        return Ok((format!("[{}]", host.to_ascii_lowercase()), port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty() || host.contains(':') {
        return Err(Error::InvalidUrl);
    }
    if !host
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(Error::InvalidUrl);
    }
    let port = match port {
        Some(port) => parse_port(&format!(":{port}"), scheme)?,
        None => scheme.default_port(),
    };
    Ok((host.to_ascii_lowercase(), port))
}

fn parse_port(suffix: &str, scheme: Scheme) -> Result<u16, Error> {
    if suffix.is_empty() {
        return Ok(scheme.default_port());
    }
    let Some(digits) = suffix.strip_prefix(':') else {
        return Err(Error::InvalidUrl);
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidUrl);
    }
    let port: u16 = digits.parse().map_err(|_| Error::InvalidUrl)?;
    if port == 0 {
        return Err(Error::InvalidUrl);
    }
    Ok(port)
}

fn split_query(reference: &str) -> (&str, Option<&str>) {
    match reference.find('?') {
        Some(index) => (&reference[..index], Some(&reference[index + 1..])),
        None => (reference, None),
    }
}

fn join_query(path: String, query: Option<&str>) -> String {
    match query {
        Some(query) => format!("{path}?{query}"),
        None => path,
    }
}

/// RFC 3986 section 5.2.4 dot-segment removal, applied to a path without a query.
///
/// Empty interior segments are preserved so a server that distinguishes `//` from `/` keeps its
/// path; only the leading root and one trailing slash are structural.
fn remove_dot_segments(path: &str) -> String {
    let segments: alloc::vec::Vec<&str> = path.split('/').collect();
    let absolute = path.starts_with('/');
    let mut output: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
    let mut trailing = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    let count = segments.len();
    for (index, segment) in segments.into_iter().enumerate() {
        let last = index + 1 == count;
        match segment {
            "." => {}
            ".." => {
                output.pop();
            }
            "" if index == 0 && absolute => {}
            "" if last => trailing = true,
            value => output.push(value),
        }
    }
    let mut result = String::new();
    if absolute {
        result.push('/');
    }
    result.push_str(&output.join("/"));
    if trailing && !result.ends_with('/') {
        result.push('/');
    }
    if result.is_empty() {
        result.push('/');
    }
    result
}

#[cfg(test)]
#[path = "url_ut.rs"]
mod tests;
