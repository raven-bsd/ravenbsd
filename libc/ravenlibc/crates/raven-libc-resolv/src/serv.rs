// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `__lookup_serv`: the numeric and `/etc/services` service lookup.

use alloc::vec::Vec;

use raven_libc_abi::errno::{EACCES, ENOENT, ENOTDIR};
use raven_libc_nss::FileSource;

use crate::consts::{
    AI_NUMERICSERV, DB_LINE_MAX, IPPROTO_TCP, IPPROTO_UDP, MAXSERVS, SOCK_DGRAM, SOCK_STREAM,
};
use crate::errors::Eai;
use crate::linereader::{LineReader, strip_comment, words};

/// One service record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    /// The port in host byte order.
    pub port: u16,
    /// `IPPROTO_TCP` or `IPPROTO_UDP`.
    pub proto: i32,
    /// `SOCK_STREAM` or `SOCK_DGRAM`.
    pub socktype: i32,
}

fn parse_port(text: &[u8]) -> Option<u32> {
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
    if used == 0 { None } else { Some(value) }
}

fn numeric(port: u32, proto: i32) -> Vec<Service> {
    let mut out = Vec::new();
    if proto != IPPROTO_UDP {
        out.push(Service {
            port: u16::try_from(port).unwrap_or(0),
            proto: IPPROTO_TCP,
            socktype: SOCK_STREAM,
        });
    }
    if proto != IPPROTO_TCP {
        out.push(Service {
            port: u16::try_from(port).unwrap_or(0),
            proto: IPPROTO_UDP,
            socktype: SOCK_DGRAM,
        });
    }
    out
}

/// `__lookup_serv`: resolve a service name or a numeric port.
pub fn lookup_serv(
    fs: &dyn FileSource,
    name: Option<&str>,
    proto: i32,
    socktype: i32,
    flags: i32,
) -> Result<Vec<Service>, Eai> {
    let mut proto = proto;
    match socktype {
        SOCK_STREAM => match proto {
            0 => proto = IPPROTO_TCP,
            IPPROTO_TCP => {}
            _ => return Err(Eai::Service),
        },
        SOCK_DGRAM => match proto {
            0 => proto = IPPROTO_UDP,
            IPPROTO_UDP => {}
            _ => return Err(Eai::Service),
        },
        0 => {}
        _ => {
            if name.is_some() {
                return Err(Eai::Service);
            }
            return Ok(alloc::vec![Service {
                port: 0,
                proto,
                socktype,
            }]);
        }
    }

    if let Some(text) = name {
        if text.is_empty() {
            return Err(Eai::Service);
        }
        let bytes = text.as_bytes();
        if bytes.iter().all(|byte| byte.is_ascii_digit()) {
            let Some(port) = parse_port(bytes) else {
                return Err(Eai::Service);
            };
            if port <= 65535 {
                return Ok(numeric(port, proto));
            }
            return Err(Eai::Service);
        }
        if flags & AI_NUMERICSERV != 0 {
            return Err(Eai::NoName);
        }
    }

    if name.is_none() {
        return Ok(numeric(0, proto));
    }
    let file = match fs.read_file("/etc/services", crate::consts::FILE_READ_MAX) {
        Ok(bytes) => bytes,
        Err(errno) if errno == ENOENT || errno == ENOTDIR || errno == EACCES => {
            return Err(Eai::Service);
        }
        Err(_) => return Err(Eai::System),
    };
    let Some(text) = name else {
        return Err(Eai::Service);
    };
    let mut out: Vec<Service> = Vec::new();
    let mut reader = LineReader::new(&file, DB_LINE_MAX);
    while let Some(line) = reader.next_line() {
        if out.len() >= MAXSERVS {
            break;
        }
        let line = strip_comment(line);
        let mut tokens = words(line);
        let Some(canonical) = tokens.next() else {
            continue;
        };
        let Some(port_proto) = tokens.next() else {
            continue;
        };
        let mut found = canonical == text.as_bytes();
        if !found {
            for alias in line
                .split(|byte| byte.is_ascii_whitespace())
                .filter(|token| !token.is_empty())
                .skip(2)
            {
                if alias == text.as_bytes() {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            continue;
        }
        let mut parts = port_proto.split(|byte| *byte == b'/');
        let Some(port_text) = parts.next() else {
            continue;
        };
        let Some(proto_text) = parts.next() else {
            continue;
        };
        let Some(port) = parse_port(port_text) else {
            continue;
        };
        if port > 65535 {
            continue;
        }
        if proto_text.starts_with(b"udp") {
            if proto == IPPROTO_TCP {
                continue;
            }
            out.push(Service {
                port: u16::try_from(port).unwrap_or(0),
                proto: IPPROTO_UDP,
                socktype: SOCK_DGRAM,
            });
        } else if proto_text.starts_with(b"tcp") {
            if proto == IPPROTO_UDP {
                continue;
            }
            out.push(Service {
                port: u16::try_from(port).unwrap_or(0),
                proto: IPPROTO_TCP,
                socktype: SOCK_STREAM,
            });
        }
    }
    if out.is_empty() {
        Err(Eai::Service)
    } else {
        Ok(out)
    }
}

#[cfg(test)]
#[path = "serv_ut.rs"]
mod tests;
