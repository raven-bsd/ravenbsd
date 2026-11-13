// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getnameinfo`: reverse hosts, one PTR query and the numeric fallback.

use alloc::string::String;

use raven_libc_abi::errno::{EACCES, ENOENT, ENOTDIR};
use raven_libc_nss::Source;

use crate::conf::parse_resolv_conf;
use crate::consts::{
    NI_DGRAM, NI_NAMEREQD, NI_NOFQDN, NI_NUMERICHOST, NI_NUMERICSCOPE, NI_NUMERICSERV, RR_PTR,
};
use crate::errors::Eai;
use crate::inet::inet_ntop;
use crate::linereader::{LineReader, strip_comment, words};
use crate::literal;
use crate::lookup::HostOrder;
use crate::msg::{dns_parse_offset, res_mkquery};
use crate::name::{dn_expand, is_valid_hostname};
use crate::traits::Env;
use crate::types::{Family, IpAddr, SockAddr};

fn target16(addr: &SockAddr) -> [u8; 16] {
    match addr.ip {
        IpAddr::V4(bytes) => IpAddr::V4(bytes).to_v4_mapped().octets16(),
        IpAddr::V6(bytes) => bytes,
    }
}

fn reverse_hosts(addr: &SockAddr, env: &Env) -> Option<String> {
    let file = env.read("/etc/hosts").ok()?;
    let target = target16(addr);
    let mut reader = LineReader::new(&file, crate::consts::HOSTS_LINE_MAX);
    while let Some(line) = reader.next_line() {
        let line = strip_comment(line);
        let mut tokens = words(line);
        let Some(text) = tokens.next() else {
            continue;
        };
        let Ok(text) = core::str::from_utf8(text) else {
            continue;
        };
        let Ok(Some(literal)) =
            literal::lookup_ipliteral(text.as_bytes(), crate::consts::AF_UNSPEC, env.ifnames)
        else {
            continue;
        };
        let candidate = literal.ip();
        let bytes = match candidate {
            IpAddr::V4(v4) => IpAddr::V4(v4).to_v4_mapped().octets16(),
            IpAddr::V6(v6) => v6,
        };
        if bytes != target || literal.scope != addr.scope {
            continue;
        }
        if let Some(name) = tokens.next()
            && let Ok(name) = core::str::from_utf8(name)
            && !name.is_empty()
            && name.len() <= crate::consts::NI_MAXHOST
        {
            return Some(String::from(name));
        }
    }
    None
}

/// The PTR name of an address; a v4-mapped IPv6 address uses the v4 name.
#[must_use]
pub fn ptr_name(addr: &SockAddr) -> String {
    match addr.ip {
        IpAddr::V4(bytes) => reverse_v4(&bytes),
        IpAddr::V6(bytes) => match IpAddr::V6(bytes).as_v4_mapped() {
            Some(v4) => reverse_v4(&v4),
            None => reverse_v6(&bytes),
        },
    }
}

fn reverse_v4(bytes: &[u8; 4]) -> String {
    alloc::format!(
        "{}.{}.{}.{}.in-addr.arpa",
        bytes.get(3).copied().unwrap_or(0),
        bytes.get(2).copied().unwrap_or(0),
        bytes.get(1).copied().unwrap_or(0),
        bytes.first().copied().unwrap_or(0)
    )
}

fn reverse_v6(bytes: &[u8; 16]) -> String {
    let digits = b"0123456789abcdef";
    let mut name = String::with_capacity(74);
    for index in (0..16usize).rev() {
        let byte = bytes.get(index).copied().unwrap_or(0);
        let low = *digits.get(usize::from(byte & 15)).unwrap_or(&b'0');
        let high = *digits
            .get(usize::from(byte.wrapping_shr(4)))
            .unwrap_or(&b'0');
        name.push(char::from(low));
        name.push('.');
        name.push(char::from(high));
        name.push('.');
    }
    name.push_str("ip6.arpa");
    name
}

fn ptr_lookup(addr: &SockAddr, env: &Env) -> Option<String> {
    let name = ptr_name(addr);
    let mut query = [0u8; 280];
    let mut id_bytes = [0u8; 2];
    if !env.random(&mut id_bytes) {
        return None;
    }
    let qlen = res_mkquery(
        0,
        name.as_bytes(),
        1,
        u16::from(RR_PTR),
        u16::from_be_bytes(id_bytes),
        &mut query,
    )?;
    let conf = parse_resolv_conf(env.read("/etc/resolv.conf").ok().as_deref());
    let ext = crate::conf::parse_options_ext(&[]);
    let mut reply = alloc::vec![0u8; 512];
    let queries: [&[u8]; 1] = [query.get(..qlen)?];
    let mut answers: [&mut [u8]; 1] = [reply.as_mut_slice()];
    let mut alens = [0usize; 1];
    crate::engine::msend(env, &conf, &ext, &queries, &mut answers, &mut alens).ok()?;
    let len = *alens.first()?;
    if len == 0 {
        return None;
    }
    let packet = reply.get(..len.min(reply.len()))?;
    let mut found = String::new();
    let _ = dns_parse_offset(packet, |rr, rdata, full, offset| {
        let _ = rdata;
        if rr != RR_PTR {
            return 0;
        }
        let mut name = [0u8; 256];
        if let Some((_, text_len)) = dn_expand(full, offset, &mut name)
            && let Ok(text) = core::str::from_utf8(name.get(..text_len).unwrap_or(&[]))
            && is_valid_hostname(text)
        {
            found = String::from(text);
        }
        0
    });
    if found.is_empty() { None } else { Some(found) }
}

fn reverse_services(port: u16, dgram: bool, env: &Env) -> Option<String> {
    let file = match env.read("/etc/services") {
        Ok(bytes) => bytes,
        Err(errno) if errno == ENOENT || errno == ENOTDIR || errno == EACCES => return None,
        Err(_) => return None,
    };
    let mut reader = LineReader::new(&file, crate::consts::DB_LINE_MAX);
    while let Some(line) = reader.next_line() {
        let line = strip_comment(line);
        let mut tokens = words(line);
        let Some(name) = tokens.next() else {
            continue;
        };
        let Some(port_proto) = tokens.next() else {
            continue;
        };
        let mut parts = port_proto.split(|byte| *byte == b'/');
        let Some(port_text) = parts.next() else {
            continue;
        };
        let Some(proto) = parts.next() else {
            continue;
        };
        let Ok(port_text) = core::str::from_utf8(port_text) else {
            continue;
        };
        let Ok(value) = port_text.parse::<u32>() else {
            continue;
        };
        if value != u32::from(port) {
            continue;
        }
        let wanted = if dgram { b"udp" } else { b"tcp" };
        if proto != wanted {
            continue;
        }
        let (Ok(text), true) = (core::str::from_utf8(name), name.len() <= 32) else {
            continue;
        };
        if !text.is_empty() {
            return Some(String::from(text));
        }
    }
    None
}

fn numeric_host(addr: &SockAddr, flags: i32, env: &Env) -> String {
    let mut text = String::new();
    let af = match addr.ip {
        IpAddr::V4(_) => crate::consts::AF_INET,
        IpAddr::V6(_) => crate::consts::AF_INET6,
    };
    let bytes = match addr.ip {
        IpAddr::V4(value) => value.to_vec(),
        IpAddr::V6(value) => value.to_vec(),
    };
    let mut buffer = [0u8; 64];
    if let Ok(len) = inet_ntop(af, &bytes, &mut buffer)
        && let Some(slice) = buffer.get(..len)
        && let Ok(value) = core::str::from_utf8(slice)
    {
        text.push_str(value);
    }
    if addr.scope != 0 && addr.family() == Family::V6 {
        text.push('%');
        let link_local = match addr.ip {
            IpAddr::V6(value) => {
                IpAddr::V6(value).is_link_local() || IpAddr::V6(value).is_mc_link_local()
            }
            IpAddr::V4(_) => false,
        };
        let named = flags & NI_NUMERICSCOPE == 0 && link_local;
        text.push_str(&literal::scope_text(addr.scope, env.ifnames, !named));
    }
    text
}

fn truncate_fqdn(name: String) -> String {
    match name.find('.') {
        Some(index) if index > 0 => String::from(name.get(..index).unwrap_or(&name)),
        _ => name,
    }
}

/// `getnameinfo`.
pub fn getnameinfo(
    addr: &SockAddr,
    hostlen: usize,
    servlen: usize,
    flags: i32,
    env: &Env,
    order: &HostOrder,
) -> Result<(Option<String>, Option<String>), Eai> {
    let mut host: Option<String> = None;
    if hostlen > 0 {
        let mut name: Option<String> = None;
        if flags & NI_NUMERICHOST == 0 {
            for entry in order.iter() {
                match entry.source {
                    Source::Skipped => {}
                    Source::Files => {
                        name = reverse_hosts(addr, env);
                    }
                    Source::Dns => {
                        name = ptr_lookup(addr, env);
                    }
                }
                if name.is_some() {
                    break;
                }
                if entry.on_notfound_return {
                    break;
                }
            }
        }
        let mut numeric = false;
        if name.is_none() {
            if flags & NI_NAMEREQD != 0 {
                return Err(Eai::NoName);
            }
            name = Some(numeric_host(addr, flags, env));
            numeric = true;
        }
        let mut text = name.unwrap_or_default();
        if flags & NI_NOFQDN != 0 && !numeric {
            text = truncate_fqdn(text);
        }
        if text.len() >= hostlen {
            return Err(Eai::Overflow);
        }
        host = Some(text);
    }

    let mut service: Option<String> = None;
    if servlen > 0 {
        let mut text = String::new();
        if flags & NI_NUMERICSERV == 0
            && let Some(name) = reverse_services(addr.port, flags & NI_DGRAM != 0, env)
        {
            text.push_str(&name);
        }
        if text.is_empty() {
            text = alloc::format!("{}", addr.port);
        }
        if text.len() >= servlen {
            return Err(Eai::Overflow);
        }
        service = Some(text);
    }
    Ok((host, service))
}

#[cfg(test)]
#[path = "nameinfo_ut.rs"]
mod tests;
