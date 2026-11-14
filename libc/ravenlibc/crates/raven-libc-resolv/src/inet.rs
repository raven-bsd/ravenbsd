// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD clean-room Rust implementation of the behaviour documented for
// musl src/network/inet_aton.c, inet_addr.c, inet_ntoa.c, inet_legacy.c,
// inet_pton.c, inet_ntop.c and the byte-order helpers. The donor files are
// MIT (musl/COPYRIGHT); the logic was read for behaviour and rewritten in
// Rust with bounded indexing. No import row is claimed for this task.

//! The `inet_*` family, byte-order helpers and the IPv6 constants.
//!
//! `inet_aton` (the BSD legacy grammar) and `inet_pton` (the strict RFC 4291 grammar) are
//! deliberately separate functions (A-net-16): `1.2.3`, `0x7f.1` and `010.0.0.1` are addresses
//! to the former and errors to the latter.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_abi::errno::{EAFNOSUPPORT, ENOSPC};

use crate::consts::{AF_INET, AF_INET6};

/// `in6addr_any`: the unspecified IPv6 address.
pub const IN6ADDR_ANY: [u8; 16] = [0u8; 16];

/// `in6addr_loopback`: `::1`.
pub const IN6ADDR_LOOPBACK: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];

/// `htons`: host to network 16-bit.
#[must_use]
pub const fn htons(value: u16) -> u16 {
    value.to_be()
}

/// `ntohs`: network to host 16-bit.
#[must_use]
pub const fn ntohs(value: u16) -> u16 {
    u16::from_be(value)
}

/// `htonl`: host to network 32-bit.
#[must_use]
pub const fn htonl(value: u32) -> u32 {
    value.to_be()
}

/// `ntohl`: network to host 32-bit.
#[must_use]
pub const fn ntohl(value: u32) -> u32 {
    u32::from_be(value)
}

/// Parse one `strtoul(..., base 0)` part; returns the value and the bytes consumed.
fn parse_legacy_number(s: &[u8]) -> Option<(u64, usize)> {
    let first = *s.first()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let (radix, mut index, mut digits) = if first == b'0' {
        match (s.get(1).copied(), s.get(2).copied()) {
            (Some(b'x' | b'X'), Some(digit)) if digit.is_ascii_hexdigit() => {
                (16u64, 2usize, 0usize)
            }
            _ => (8u64, 1usize, 1usize),
        }
    } else {
        (10u64, 0usize, 0usize)
    };
    let mut value = 0u64;
    while let Some(byte) = s.get(index).copied() {
        let digit = match byte {
            b'0'..=b'9' => u64::from(byte.saturating_sub(b'0')),
            b'a'..=b'f' => u64::from(byte.saturating_sub(b'a')).saturating_add(10),
            b'A'..=b'F' => u64::from(byte.saturating_sub(b'A')).saturating_add(10),
            _ => break,
        };
        if digit >= radix {
            break;
        }
        value = value.saturating_mul(radix).saturating_add(digit);
        digits = digits.saturating_add(1);
        index = index.saturating_add(1);
    }
    if digits == 0 {
        return None;
    }
    Some((value, index))
}

/// `inet_aton`: the BSD legacy grammar `a`, `a.b`, `a.b.c`, `a.b.c.d` in decimal, octal and hex.
#[must_use]
pub fn inet_aton(s: &[u8]) -> Option<[u8; 4]> {
    let mut parts = [0u64; 4];
    let mut count = 0usize;
    let mut pos = 0usize;
    loop {
        let (value, used) = parse_legacy_number(s.get(pos..)?)?;
        if used == 0 {
            return None;
        }
        *parts.get_mut(count)? = value;
        pos = pos.checked_add(used)?;
        match s.get(pos).copied() {
            None => break,
            Some(b'.') => {
                count = count.checked_add(1)?;
                if count >= 4 {
                    return None;
                }
                pos = pos.checked_add(1)?;
            }
            Some(_) => return None,
        }
    }
    match count {
        0 => {
            let low = parts.first().copied().unwrap_or(0) & 0x00ff_ffff;
            let high = parts.first().copied().unwrap_or(0) >> 24;
            if let Some(slot) = parts.get_mut(1) {
                *slot = low;
            }
            if let Some(slot) = parts.get_mut(0) {
                *slot = high;
            }
            let low = parts.get(1).copied().unwrap_or(0) & 0x0000_ffff;
            let high = parts.get(1).copied().unwrap_or(0) >> 16;
            if let Some(slot) = parts.get_mut(2) {
                *slot = low;
            }
            if let Some(slot) = parts.get_mut(1) {
                *slot = high;
            }
            let low = parts.get(2).copied().unwrap_or(0) & 0x0000_00ff;
            let high = parts.get(2).copied().unwrap_or(0) >> 8;
            if let Some(slot) = parts.get_mut(3) {
                *slot = low;
            }
            if let Some(slot) = parts.get_mut(2) {
                *slot = high;
            }
        }
        1 => {
            let low = parts.get(1).copied().unwrap_or(0) & 0x0000_ffff;
            let high = parts.get(1).copied().unwrap_or(0) >> 16;
            if let Some(slot) = parts.get_mut(2) {
                *slot = low;
            }
            if let Some(slot) = parts.get_mut(1) {
                *slot = high;
            }
            let low = parts.get(2).copied().unwrap_or(0) & 0x0000_00ff;
            let high = parts.get(2).copied().unwrap_or(0) >> 8;
            if let Some(slot) = parts.get_mut(3) {
                *slot = low;
            }
            if let Some(slot) = parts.get_mut(2) {
                *slot = high;
            }
        }
        2 => {
            let low = parts.get(2).copied().unwrap_or(0) & 0x0000_00ff;
            let high = parts.get(2).copied().unwrap_or(0) >> 8;
            if let Some(slot) = parts.get_mut(3) {
                *slot = low;
            }
            if let Some(slot) = parts.get_mut(2) {
                *slot = high;
            }
        }
        _ => {}
    }
    let mut out = [0u8; 4];
    for (slot, value) in out.iter_mut().zip(parts.iter()) {
        if *value > 255 {
            return None;
        }
        *slot = u8::try_from(*value).ok()?;
    }
    Some(out)
}

/// `inet_addr`: the address as the C integer convention, or `INADDR_NONE` (all ones).
#[must_use]
pub fn inet_addr(s: &[u8]) -> u32 {
    match inet_aton(s) {
        Some(a) => u32::from_ne_bytes(a),
        None => u32::MAX,
    }
}

/// `inet_ntoa`: the dotted quad. The shell owns the thread-local buffer (T-12-15).
#[must_use]
pub fn inet_ntoa(addr: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
}

/// `inet_network`: the network part as a host-order number.
#[must_use]
pub fn inet_network(s: &[u8]) -> u32 {
    ntohl(inet_addr(s))
}

/// `inet_makeaddr`: combine a network and a host part.
#[must_use]
pub fn inet_makeaddr(net: u32, host: u32) -> u32 {
    if net < 256 {
        host | (net << 24)
    } else if net < 65536 {
        host | (net << 16)
    } else {
        host | (net << 8)
    }
}

/// `inet_lnaof`: the local network address part.
#[must_use]
pub fn inet_lnaof(in_addr: u32) -> u32 {
    if (in_addr >> 24) < 128 {
        in_addr & 0x00ff_ffff
    } else if (in_addr >> 24) < 192 {
        in_addr & 0x0000_ffff
    } else {
        in_addr & 0x0000_00ff
    }
}

/// `inet_netof`: the network number.
#[must_use]
pub fn inet_netof(in_addr: u32) -> u32 {
    if (in_addr >> 24) < 128 {
        in_addr >> 24
    } else if (in_addr >> 24) < 192 {
        in_addr >> 16
    } else {
        in_addr >> 8
    }
}

/// `inet_pton`: `1`, `0` (invalid text) or `-1` for an unsupported family.
pub fn inet_pton(af: i32, s: &[u8], out: &mut [u8]) -> i32 {
    if af == AF_INET {
        return match pton4(s) {
            Some(addr) => {
                match out.get_mut(..4) {
                    Some(slot) => slot.copy_from_slice(&addr),
                    None => return 0,
                }
                1
            }
            None => 0,
        };
    }
    if af != AF_INET6 {
        return -1;
    }
    match pton6(s) {
        Some(addr) => {
            match out.get_mut(..16) {
                Some(slot) => slot.copy_from_slice(&addr),
                None => return 0,
            }
            1
        }
        None => 0,
    }
}

/// The strict IPv4 grammar: at most three digits per octet, no leading zero.
fn pton4(s: &[u8]) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut pos = 0usize;
    for index in 0..4usize {
        let mut j = 0usize;
        let mut value = 0u32;
        while j < 3 {
            let Some(byte) = s.get(pos.checked_add(j)?).copied() else {
                break;
            };
            if !byte.is_ascii_digit() {
                break;
            }
            value = value
                .saturating_mul(10)
                .saturating_add(u32::from(byte.saturating_sub(b'0')));
            j = j.saturating_add(1);
        }
        if j == 0 {
            return None;
        }
        let first = s.get(pos).copied()?;
        if j > 1 && first == b'0' {
            return None;
        }
        if value > 255 {
            return None;
        }
        if let Some(slot) = out.get_mut(index) {
            *slot = u8::try_from(value).ok()?;
        }
        let next = s.get(pos.checked_add(j)?).copied();
        if next.is_none() && index == 3 {
            return Some(out);
        }
        if next != Some(b'.') {
            return None;
        }
        pos = pos.checked_add(j)?.checked_add(1)?;
    }
    None
}

/// Hex digit value.
fn hexval(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte.saturating_sub(b'0'))),
        b'a'..=b'f' => Some(u32::from(byte.saturating_sub(b'a')).saturating_add(10)),
        b'A'..=b'F' => Some(u32::from(byte.saturating_sub(b'A')).saturating_add(10)),
        _ => None,
    }
}

/// The IPv6 grammar including `::` and an embedded IPv4 tail.
fn pton6(s0: &[u8]) -> Option<[u8; 16]> {
    let mut s = s0;
    if s.first().copied() == Some(b':') {
        if s.get(1).copied() != Some(b':') {
            return None;
        }
        s = s.get(1..)?;
    }
    let mut ip = [0u16; 8];
    let mut i = 0usize;
    let mut brk: Option<usize> = None;
    let mut tail: Option<&[u8]> = None;
    loop {
        if s.first().copied() == Some(b':') && brk.is_none() {
            brk = Some(i);
            if let Some(slot) = ip.get_mut(i & 7) {
                *slot = 0;
            }
            s = s.get(1..)?;
            if s.is_empty() {
                break;
            }
            if i == 7 {
                return None;
            }
            i = i.checked_add(1)?;
            continue;
        }
        let mut j = 0usize;
        let mut value = 0u32;
        while j < 4 {
            let Some(digit) = s.get(j).copied().and_then(hexval) else {
                break;
            };
            value = value.saturating_mul(16).saturating_add(digit);
            j = j.saturating_add(1);
        }
        if j == 0 {
            return None;
        }
        if let Some(slot) = ip.get_mut(i & 7) {
            *slot = u16::try_from(value).ok()?;
        }
        let next = s.get(j).copied();
        if next.is_none() && (brk.is_some() || i == 7) {
            break;
        }
        if i == 7 {
            return None;
        }
        match next {
            Some(b':') => {
                s = s.get(j.checked_add(1)?..)?;
                i = i.checked_add(1)?;
            }
            Some(b'.') => {
                if i < 6 && brk.is_none() {
                    return None;
                }
                i = i.checked_add(1)?;
                if let Some(slot) = ip.get_mut(i & 7) {
                    *slot = 0;
                }
                tail = Some(s);
                break;
            }
            _ => return None,
        }
    }
    if let Some(break_at) = brk {
        let count = i.checked_add(1)?.checked_sub(break_at)?;
        let dest = break_at.checked_add(7)?.checked_sub(i)?;
        // The destination can overlap the source (for example `0::a:4e:3` moves groups one
        // slot to the right). `copy_within` has memmove semantics; a forward element loop would
        // overwrite the source and zero the last group (C70 FINDING 1).
        ip.copy_within(break_at..break_at.checked_add(count)?, dest);
        for slot in ip.iter_mut().skip(break_at).take(7usize.saturating_sub(i)) {
            *slot = 0;
        }
    }
    let mut out = [0u8; 16];
    for (index, group) in ip.iter().enumerate() {
        let hi = u8::try_from(group >> 8).ok()?;
        let lo = u8::try_from(group & 0xff).ok()?;
        if let Some(slot) = out.get_mut(index.saturating_mul(2)) {
            *slot = hi;
        }
        if let Some(slot) = out.get_mut(index.saturating_mul(2).saturating_add(1)) {
            *slot = lo;
        }
    }
    if let Some(v4) = tail {
        let parsed = pton4(v4)?;
        let start = out.len().checked_sub(4)?;
        if let Some(slot) = out.get_mut(start..) {
            slot.copy_from_slice(&parsed);
        }
    }
    Some(out)
}

/// `inet_ntop`: format an address; `ENOSPC` when `out` is too small.
pub fn inet_ntop(af: i32, a: &[u8], out: &mut [u8]) -> Result<usize, crate::Errno> {
    match af {
        AF_INET => {
            if a.len() < 4 {
                return Err(EAFNOSUPPORT);
            }
            let text = format!(
                "{}.{}.{}.{}",
                a.first().copied().unwrap_or(0),
                a.get(1).copied().unwrap_or(0),
                a.get(2).copied().unwrap_or(0),
                a.get(3).copied().unwrap_or(0)
            );
            copy_text(text.as_bytes(), out)
        }
        AF_INET6 => {
            if a.len() < 16 {
                return Err(EAFNOSUPPORT);
            }
            let text = format_v6(a);
            copy_text(text.as_bytes(), out)
        }
        _ => Err(EAFNOSUPPORT),
    }
}

fn copy_text(text: &[u8], out: &mut [u8]) -> Result<usize, crate::Errno> {
    if out.len() < text.len() {
        return Err(ENOSPC);
    }
    if let Some(slot) = out.get_mut(..text.len()) {
        slot.copy_from_slice(text);
    }
    Ok(text.len())
}

fn format_v6(a: &[u8]) -> String {
    let mapped = a
        .get(..10)
        .is_some_and(|head| head.iter().all(|byte| *byte == 0))
        && a.get(10).copied() == Some(0xff)
        && a.get(11).copied() == Some(0xff);
    let mut text = String::with_capacity(46);
    if mapped {
        text.push_str(&format!(
            "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{}.{}.{}.{}",
            group(a, 0),
            group(a, 2),
            group(a, 4),
            group(a, 6),
            group(a, 8),
            group(a, 10),
            a.get(12).copied().unwrap_or(0),
            a.get(13).copied().unwrap_or(0),
            a.get(14).copied().unwrap_or(0),
            a.get(15).copied().unwrap_or(0)
        ));
    } else {
        text.push_str(&format!(
            "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
            group(a, 0),
            group(a, 2),
            group(a, 4),
            group(a, 6),
            group(a, 8),
            group(a, 10),
            group(a, 12),
            group(a, 14)
        ));
    }
    compress_zeros(text)
}

fn group(a: &[u8], index: usize) -> u16 {
    let hi = u16::from(a.get(index).copied().unwrap_or(0));
    let lo = u16::from(a.get(index.saturating_add(1)).copied().unwrap_or(0));
    hi.saturating_mul(256).saturating_add(lo)
}

/// Replace the longest `:(0:)+` run, preferring the first on a tie and treating a leading run
/// as one character shorter (the musl rule).
fn compress_zeros(text: String) -> String {
    let bytes = text.into_bytes();
    let mut best = 0usize;
    let mut max = 2usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let candidate = i == 0 || bytes.get(i).copied() == Some(b':');
        if candidate {
            let mut j = 0usize;
            while let Some(byte) = bytes.get(i.saturating_add(j)).copied() {
                if byte == b':' || byte == b'0' {
                    j = j.saturating_add(1);
                } else {
                    break;
                }
            }
            let handicap = usize::from(best == 0);
            if j > max.saturating_add(handicap) {
                best = i;
                max = j;
            }
        }
        i = i.saturating_add(1);
    }
    if max <= 3 {
        return String::from_utf8(bytes).unwrap_or_default();
    }
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    if let Some(head) = bytes.get(..best) {
        out.extend_from_slice(head);
    }
    out.extend_from_slice(b"::");
    if let Some(tail) = bytes.get(best.saturating_add(max)..) {
        out.extend_from_slice(tail);
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
#[path = "inet_ut.rs"]
mod tests;
