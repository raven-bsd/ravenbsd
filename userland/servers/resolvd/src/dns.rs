// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded DNS wire codec for the resolver.
//!
//! Only the records the service needs are decoded: A, AAAA and CNAME.  Every
//! length and compression pointer is checked; a malformed packet is an error,
//! never a guessed answer.

use alloc::string::String;
use alloc::vec::Vec;

/// Maximum DNS message size accepted from a server.
pub const MAX_PACKET: usize = 512;
/// Type A.
pub const TYPE_A: u16 = 1;
/// Type CNAME.
pub const TYPE_CNAME: u16 = 5;
/// Type AAAA.
pub const TYPE_AAAA: u16 = 28;
/// Internet class.
pub const CLASS_IN: u16 = 1;
const HEADER_LEN: usize = 12;
const RCODE_MASK: u16 = 0x000f;
const RCODE_NXDOMAIN: u16 = 3;
const MAX_ANSWERS: usize = 16;
const MAX_POINTER_JUMPS: usize = 16;

/// A DNS codec error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnsError {
    /// The message ended before a field.
    TooShort,
    /// The header is not a standard response or has the wrong id.
    BadHeader,
    /// A name is too long, has a bad label or a pointer loop.
    BadName,
    /// A record has a bad length or type.
    BadRecord,
    /// The server returned an RCODE other than NOERROR/NXDOMAIN.
    ServerError(u8),
}

/// One address answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Address {
    /// A record.
    V4([u8; 4]),
    /// AAAA record.
    V6([u8; 16]),
}

/// One decoded answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// An A or AAAA record.
    Address {
        /// TTL in seconds.
        ttl: u32,
        /// Address bytes.
        address: Address,
    },
    /// A CNAME target.
    Cname(String),
}

/// A decoded DNS response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// NOERROR (0) or NXDOMAIN (3), after validation.
    pub rcode: u8,
    /// Answers in server order.
    pub answers: Vec<Answer>,
}

impl Response {
    /// The first address of the requested family, if present.
    pub fn address(&self) -> Option<(u32, Address)> {
        for answer in &self.answers {
            if let Answer::Address { ttl, address } = answer {
                return Some((*ttl, *address));
            }
        }
        None
    }

    /// The first CNAME target, if present.
    pub fn cname(&self) -> Option<&str> {
        self.answers.iter().find_map(|answer| match answer {
            Answer::Cname(name) => Some(name.as_str()),
            _ => None,
        })
    }
}

/// Build a standard recursive query.
pub fn build_query(id: u16, name: &str, qtype: u16) -> Result<Vec<u8>, DnsError> {
    if !matches!(qtype, TYPE_A | TYPE_AAAA) {
        return Err(DnsError::BadRecord);
    }
    let trimmed = name.strip_suffix('.').unwrap_or(name);
    if trimmed.is_empty() || trimmed.len() > 253 || trimmed.contains('\0') {
        return Err(DnsError::BadName);
    }
    let mut packet = Vec::with_capacity(HEADER_LEN + name.len() + 6);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&0x0100u16.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    for label in trimmed.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label.is_ascii()
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(DnsError::BadName);
        }
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0);
    packet.extend_from_slice(&qtype.to_be_bytes());
    packet.extend_from_slice(&CLASS_IN.to_be_bytes());
    Ok(packet)
}

/// Parse a DNS response, requiring `expected_id` and a standard response.
pub fn parse_response(packet: &[u8], expected_id: u16) -> Result<Response, DnsError> {
    if packet.len() < HEADER_LEN || packet.len() > MAX_PACKET {
        return Err(DnsError::TooShort);
    }
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if id != expected_id || flags & 0x8000 == 0 || (flags >> 11) & 0x0f != 0 {
        return Err(DnsError::BadHeader);
    }
    let rcode = (flags & RCODE_MASK) as u8;
    if rcode != 0 && rcode != RCODE_NXDOMAIN as u8 {
        return Err(DnsError::ServerError(rcode));
    }
    let questions = usize::from(u16::from_be_bytes([packet[4], packet[5]]));
    let answers = usize::from(u16::from_be_bytes([packet[6], packet[7]]));
    if questions > 8 || answers > MAX_ANSWERS {
        return Err(DnsError::BadRecord);
    }
    let mut offset = HEADER_LEN;
    for _ in 0..questions {
        read_name(packet, &mut offset)?;
        let end = offset.checked_add(4).ok_or(DnsError::TooShort)?;
        if end > packet.len() {
            return Err(DnsError::TooShort);
        }
        offset = end;
    }
    let mut decoded = Vec::new();
    for _ in 0..answers {
        read_name(packet, &mut offset)?;
        let header_end = offset.checked_add(10).ok_or(DnsError::TooShort)?;
        if header_end > packet.len() {
            return Err(DnsError::TooShort);
        }
        let kind = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let class = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        let ttl = u32::from_be_bytes([
            packet[offset + 4],
            packet[offset + 5],
            packet[offset + 6],
            packet[offset + 7],
        ]);
        let length = usize::from(u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]));
        offset = header_end;
        let end = offset.checked_add(length).ok_or(DnsError::TooShort)?;
        if end > packet.len() {
            return Err(DnsError::TooShort);
        }
        if class == CLASS_IN {
            match kind {
                TYPE_A if length == 4 => decoded.push(Answer::Address {
                    ttl,
                    address: Address::V4(
                        packet[offset..end]
                            .try_into()
                            .map_err(|_| DnsError::BadRecord)?,
                    ),
                }),
                TYPE_AAAA if length == 16 => decoded.push(Answer::Address {
                    ttl,
                    address: Address::V6(
                        packet[offset..end]
                            .try_into()
                            .map_err(|_| DnsError::BadRecord)?,
                    ),
                }),
                TYPE_CNAME if length > 0 => {
                    let mut name_offset = offset;
                    let target = read_name(packet, &mut name_offset)?;
                    decoded.push(Answer::Cname(target));
                }
                TYPE_A | TYPE_AAAA | TYPE_CNAME => return Err(DnsError::BadRecord),
                _ => {}
            }
        }
        offset = end;
    }
    Ok(Response {
        rcode,
        answers: decoded,
    })
}

fn read_name(packet: &[u8], offset: &mut usize) -> Result<String, DnsError> {
    let mut labels = Vec::new();
    let mut cursor = *offset;
    let mut end = None;
    let mut jumps = 0usize;
    loop {
        let length = *packet.get(cursor).ok_or(DnsError::TooShort)?;
        if length & 0xc0 == 0xc0 {
            let low = *packet.get(cursor + 1).ok_or(DnsError::TooShort)?;
            let pointer = (usize::from(length & 0x3f) << 8) | usize::from(low);
            if pointer >= packet.len() {
                return Err(DnsError::BadName);
            }
            if end.is_none() {
                end = Some(cursor + 2);
            }
            jumps += 1;
            if jumps > MAX_POINTER_JUMPS {
                return Err(DnsError::BadName);
            }
            cursor = pointer;
            continue;
        }
        if length & 0xc0 != 0 {
            return Err(DnsError::BadName);
        }
        if length == 0 {
            *offset = end.unwrap_or(cursor + 1);
            break;
        }
        let length = usize::from(length);
        let start = cursor + 1;
        let finish = start.checked_add(length).ok_or(DnsError::TooShort)?;
        let label = packet.get(start..finish).ok_or(DnsError::TooShort)?;
        if label.len() > 63 || !label.is_ascii() {
            return Err(DnsError::BadName);
        }
        labels.push(core::str::from_utf8(label).map_err(|_| DnsError::BadName)?);
        cursor = finish;
        if labels.iter().map(|label| label.len() + 1).sum::<usize>() > 253 {
            return Err(DnsError::BadName);
        }
    }
    Ok(labels.join("."))
}

#[cfg(test)]
#[path = "dns_ut.rs"]
mod tests;
