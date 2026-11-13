// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/etc/resolv.conf`: the nameserver list, the resolver knobs and the ring-2 extensions.

use alloc::string::String;

use crate::consts::{DNS_PORT, MAXNS, RESOLV_CONF_LINE_MAX};
use crate::inet::{inet_aton, inet_pton};
use crate::linereader::{LineReader, trim};
use crate::types::{IpAddr, SockAddr};

/// The parsed resolver configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvConf {
    /// Up to three nameservers.
    pub ns: [Option<SockAddr>; MAXNS],
    /// The number of valid nameservers.
    pub nns: usize,
    /// The total number of tries.
    pub attempts: u32,
    /// The number of dots that suppresses the search list.
    pub ndots: u32,
    /// The total timeout in seconds.
    pub timeout: u32,
    /// The search domains separated by whitespace.
    pub search: String,
}

impl ResolvConf {
    /// The donor defaults: 127.0.0.1, ndots 1, timeout 5, attempts 2.
    #[must_use]
    pub fn defaults() -> Self {
        let mut ns = [None; MAXNS];
        if let Some(slot) = ns.get_mut(0) {
            *slot = Some(SockAddr::new(IpAddr::V4([127, 0, 0, 1]), DNS_PORT));
        }
        Self {
            ns,
            nns: 1,
            attempts: 2,
            ndots: 1,
            timeout: 5,
            search: String::new(),
        }
    }

    /// The first nameserver, when configured.
    #[must_use]
    pub fn first_ns(&self) -> Option<SockAddr> {
        self.ns.first().copied().flatten()
    }
}

impl Default for ResolvConf {
    fn default() -> Self {
        Self::defaults()
    }
}

/// The `options` extensions of ring 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfExt {
    /// Send an EDNS0 OPT record (off unless requested; O-12.6, T-12-19).
    pub edns0: bool,
    /// Use TCP for every query.
    pub use_vc: bool,
    /// Accept `rotate`; the engine keeps the configured order (O-12.9).
    pub rotate: bool,
    /// Accept `single-request`.
    pub single_request: bool,
    /// Accept `trust-ad`.
    pub trust_ad: bool,
    /// Suppress AAAA queries.
    pub no_aaaa: bool,
}

/// Find `needle` in `haystack`.
fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let last = haystack.len().saturating_sub(needle.len());
    for index in 0..=last {
        if haystack.get(index..index.saturating_add(needle.len())) == Some(needle) {
            return Some(index);
        }
    }
    None
}

/// Parse a decimal number at the start of `text`; returns `None` when no digit is present.
fn parse_decimal(text: &[u8]) -> Option<(u32, usize)> {
    let mut value = 0u32;
    let mut used = 0usize;
    while let Some(byte) = text.get(used).copied() {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(byte.saturating_sub(b'0')));
        used = used.saturating_add(1);
    }
    if used == 0 { None } else { Some((value, used)) }
}

/// Parse one option value of the form `key:value`.
fn option_value(line: &[u8], key: &[u8]) -> Option<u32> {
    let index = find_sub(line, key)?;
    let start = index.checked_add(key.len())?;
    let (value, _) = parse_decimal(line.get(start..)?)?;
    Some(value)
}

/// Parse a nameserver literal; numeric scopes only (no `IfNames` here).
#[must_use]
pub fn parse_nameserver(text: &[u8]) -> Option<SockAddr> {
    if let Some(v4) = inet_aton(text) {
        return Some(SockAddr::new(IpAddr::V4(v4), DNS_PORT));
    }
    let percent = text.iter().position(|byte| *byte == b'%');
    let (host, scope_text) = match percent {
        Some(index) => (
            text.get(..index)?,
            Some(text.get(index.saturating_add(1)..)?),
        ),
        None => (text, None),
    };
    let mut addr = [0u8; 16];
    if inet_pton(crate::consts::AF_INET6, host, &mut addr) != 1 {
        return None;
    }
    let mut scope = 0u32;
    if let Some(digits) = scope_text {
        let (value, used) = parse_decimal(digits)?;
        if used != digits.len() {
            return None;
        }
        scope = value;
    }
    Some(SockAddr::with_scope(IpAddr::V6(addr), DNS_PORT, scope))
}

fn parse_options_into(conf: &mut ResolvConf, line: &[u8]) {
    if let Some(value) = option_value(line, b"ndots:") {
        conf.ndots = value.min(15);
    }
    if let Some(value) = option_value(line, b"attempts:") {
        conf.attempts = value.min(10);
    }
    if let Some(value) = option_value(line, b"timeout:") {
        conf.timeout = value.min(60);
    }
}

/// `__get_resolv_conf`: `None` means the file was unreadable and gives the donor defaults.
#[must_use]
pub fn parse_resolv_conf(file: Option<&[u8]>) -> ResolvConf {
    let mut conf = ResolvConf::defaults();
    conf.nns = 0;
    conf.ndots = 1;
    conf.attempts = 2;
    conf.timeout = 5;
    if let Some(slot) = conf.ns.get_mut(0) {
        *slot = None;
    }
    let Some(bytes) = file else {
        return ResolvConf::defaults();
    };
    let mut reader = LineReader::new(bytes, RESOLV_CONF_LINE_MAX);
    while let Some(line) = reader.next_line() {
        if line.starts_with(b"options") && line.get(7).is_some_and(u8::is_ascii_whitespace) {
            parse_options_into(&mut conf, line);
            continue;
        }
        if line.starts_with(b"nameserver") && line.get(10).is_some_and(u8::is_ascii_whitespace) {
            if conf.nns >= MAXNS {
                continue;
            }
            let rest = trim(line.get(11..).unwrap_or(&[]));
            let token = rest
                .split(|byte| byte.is_ascii_whitespace())
                .next()
                .unwrap_or(&[]);
            if let Some(server) = parse_nameserver(token)
                && let Some(slot) = conf.ns.get_mut(conf.nns)
            {
                *slot = Some(server);
                conf.nns = conf.nns.saturating_add(1);
            }
            continue;
        }
        let domain =
            line.starts_with(b"domain") && line.get(6).is_some_and(u8::is_ascii_whitespace);
        let search =
            line.starts_with(b"search") && line.get(6).is_some_and(u8::is_ascii_whitespace);
        if !domain && !search {
            continue;
        }
        let rest = trim(line.get(7..).unwrap_or(&[]));
        if rest.is_empty() {
            conf.search.clear();
        } else if let Ok(text) = core::str::from_utf8(rest) {
            conf.search.clear();
            conf.search.push_str(text);
        }
    }
    if conf.nns == 0 {
        if let Some(slot) = conf.ns.get_mut(0) {
            *slot = Some(SockAddr::new(IpAddr::V4([127, 0, 0, 1]), DNS_PORT));
        }
        conf.nns = 1;
    }
    conf
}

/// Parse the `options` lines of ring 2; unknown words are ignored.
#[must_use]
pub fn parse_options_ext(file: &[u8]) -> ConfExt {
    let mut ext = ConfExt::default();
    let mut reader = LineReader::new(file, RESOLV_CONF_LINE_MAX);
    while let Some(line) = reader.next_line() {
        if !line.starts_with(b"options") || !line.get(7).is_some_and(u8::is_ascii_whitespace) {
            continue;
        }
        for token in line
            .get(7..)
            .unwrap_or(&[])
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|token| !token.is_empty())
        {
            match token {
                b"edns0" => ext.edns0 = true,
                b"use-vc" | b"tcp" => ext.use_vc = true,
                b"rotate" => ext.rotate = true,
                b"single-request" => ext.single_request = true,
                b"trust-ad" => ext.trust_ad = true,
                b"no-aaaa" => ext.no_aaaa = true,
                _ => {}
            }
        }
    }
    ext
}

#[cfg(test)]
#[path = "conf_ut.rs"]
mod tests;
