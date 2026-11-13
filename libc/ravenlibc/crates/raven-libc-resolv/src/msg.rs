// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Query construction, answer parsing, the question check and the EDNS0 OPT record.

use crate::consts::{ABUF_SIZE, DNS_HEADER_LEN, DNS_LABEL_MAX, DNS_NAME_MAX, EDNS0_LEN, RR_OPT};
use crate::name::dn_expand;

/// Build a DNS query.
///
/// `id` comes from [`crate::traits::Entropy`] (D-12-2). The opcode goes into byte 2 as
/// `op * 8 + 1` and byte 3 carries the AD bit (`32`). Returns the query length.
pub fn res_mkquery(
    op: u8,
    name: &[u8],
    class: u16,
    rtype: u16,
    id: u16,
    buf: &mut [u8],
) -> Option<usize> {
    let mut length = name.len().min(255);
    if length > 0 && name.get(length.saturating_sub(1)).copied() == Some(b'.') {
        length = length.saturating_sub(1);
    }
    if length > 0 && name.get(length.saturating_sub(1)).copied() == Some(b'.') {
        return None;
    }
    let needed = if length == 0 {
        17usize
    } else {
        length.checked_add(18)?
    };
    if length > DNS_NAME_MAX || buf.len() < needed || op > 15 || class > 255 || rtype > 255 {
        return None;
    }
    let mut query = [0u8; 280];
    if let Some(slot) = query.get_mut(2) {
        *slot = op.wrapping_mul(8).wrapping_add(1);
    }
    if let Some(slot) = query.get_mut(3) {
        *slot = 32;
    }
    if let Some(slot) = query.get_mut(5) {
        *slot = 1;
    }
    if let Some(slot) = query.get_mut(13..13usize.checked_add(length)?) {
        slot.copy_from_slice(name.get(..length)?);
    }
    let mut i = 13usize;
    while query.get(i).copied().unwrap_or(0) != 0 {
        let mut j = i;
        while query
            .get(j)
            .copied()
            .is_some_and(|byte| byte != 0 && byte != b'.')
        {
            j = j.saturating_add(1);
        }
        let label = j.checked_sub(i)?;
        if label == 0 || label > DNS_LABEL_MAX {
            return None;
        }
        if let Some(slot) = query.get_mut(i.checked_sub(1)?) {
            *slot = u8::try_from(label).ok()?;
        }
        i = j.checked_add(1)?;
    }
    let type_at = i.checked_add(1)?;
    let class_at = i.checked_add(3)?;
    *query.get_mut(type_at)? = u8::try_from(rtype).ok()?;
    *query.get_mut(class_at)? = u8::try_from(class).ok()?;
    if let Some(slot) = query.get_mut(0) {
        *slot = u8::try_from(id >> 8).unwrap_or(0);
    }
    if let Some(slot) = query.get_mut(1) {
        *slot = u8::try_from(id & 0xff).unwrap_or(0);
    }
    let target = buf.get_mut(..needed)?;
    target.copy_from_slice(query.get(..needed)?);
    Some(needed)
}

/// Append an EDNS0 OPT pseudo-record and set `arcount` to one.
pub fn append_edns0(buf: &mut [u8], len: usize, enabled: bool) -> Option<usize> {
    if !enabled {
        return Some(len);
    }
    let new_len = len.checked_add(EDNS0_LEN)?;
    if buf.len() < new_len || len < DNS_HEADER_LEN {
        return None;
    }
    let written = pack_edns0(crate::consts::EDNS0_UDP_SIZE, false, buf.get_mut(len..)?)?;
    if written != EDNS0_LEN {
        return None;
    }
    if let Some(slot) = buf.get_mut(10) {
        *slot = 0;
    }
    if let Some(slot) = buf.get_mut(11) {
        *slot = 1;
    }
    Some(new_len)
}

/// The packet length a callback may inspect.
pub const MAX_PACKET: usize = ABUF_SIZE;

/// `__dns_parse`: walk the question and answer sections, calling `cb` for every answer RR.
///
/// The callback receives the record type, the rdata slice and the whole packet. `rcode != 0`
/// produces no records; an overrun produces `-1`.
pub fn dns_parse<F: FnMut(u8, &[u8], &[u8]) -> i32>(packet: &[u8], mut cb: F) -> i32 {
    dns_parse_offset(packet, |rr, rdata, full, _offset| cb(rr, rdata, full))
}

/// Like [`dns_parse`] but the callback also receives the rdata offset inside the packet, which
/// is what a compressed name needs.
pub fn dns_parse_offset<F: FnMut(u8, &[u8], &[u8], usize) -> i32>(packet: &[u8], mut cb: F) -> i32 {
    let len = packet.len();
    if len < DNS_HEADER_LEN {
        return -1;
    }
    if crate::u8_at(packet, 3) & 15 != 0 {
        return 0;
    }
    let mut p = DNS_HEADER_LEN;
    let mut qdcount = crate::be16_at(packet, 4).unwrap_or(0);
    let mut ancount = crate::be16_at(packet, 6).unwrap_or(0);
    while qdcount > 0 {
        qdcount = qdcount.saturating_sub(1);
        while p < len && crate::u8_at(packet, p).wrapping_sub(1) < 127 {
            p = p.saturating_add(1);
        }
        if p > len.saturating_sub(6) {
            return -1;
        }
        let pointer = crate::u8_at(packet, p) != 0;
        p = p.saturating_add(5).saturating_add(usize::from(pointer));
    }
    while ancount > 0 {
        ancount = ancount.saturating_sub(1);
        while p < len && crate::u8_at(packet, p).wrapping_sub(1) < 127 {
            p = p.saturating_add(1);
        }
        if p > len.saturating_sub(12) {
            return -1;
        }
        let pointer = crate::u8_at(packet, p) != 0;
        p = p.saturating_add(1).saturating_add(usize::from(pointer));
        let remaining = match len.checked_sub(p) {
            Some(value) => value,
            None => return -1,
        };
        if remaining < 10 {
            return -1;
        }
        let rdlen = usize::from(crate::be16_at(packet, p.saturating_add(8)).unwrap_or(0));
        if rdlen > remaining.saturating_sub(10) {
            return -1;
        }
        let start = p.saturating_add(10);
        let Some(rdata) = packet.get(start..start.saturating_add(rdlen)) else {
            return -1;
        };
        if cb(
            crate::u8_at(packet, p.saturating_add(1)),
            rdata,
            packet,
            start,
        ) < 0
        {
            return -1;
        }
        p = start.saturating_add(rdlen);
    }
    0
}

/// D-12-4: the answer must belong to the query: QR set, opcode 0, `qdcount` 1, and the same
/// ASCII case-insensitive name, type and class.
#[must_use]
pub fn question_matches(query: &[u8], answer: &[u8]) -> bool {
    if query.len() < DNS_HEADER_LEN || answer.len() < DNS_HEADER_LEN {
        return false;
    }
    if crate::u8_at(query, 2) & 0x80 != 0 {
        return false;
    }
    if crate::u8_at(answer, 2) & 0x80 == 0 {
        return false;
    }
    if (crate::u8_at(query, 2) >> 3) & 15 != 0 || (crate::u8_at(answer, 2) >> 3) & 15 != 0 {
        return false;
    }
    if crate::be16_at(query, 4) != Some(1) || crate::be16_at(answer, 4) != Some(1) {
        return false;
    }
    let mut qname = [0u8; 256];
    let mut aname = [0u8; 256];
    let Some((qconsumed, qlen)) = dn_expand(query, DNS_HEADER_LEN, &mut qname) else {
        return false;
    };
    let Some((aconsumed, alen)) = dn_expand(answer, DNS_HEADER_LEN, &mut aname) else {
        return false;
    };
    if qlen != alen {
        return false;
    }
    let qbytes = qname.get(..qlen).unwrap_or(&[]);
    let abytes = aname.get(..alen).unwrap_or(&[]);
    if !qbytes.eq_ignore_ascii_case(abytes) {
        return false;
    }
    let qpos = DNS_HEADER_LEN.saturating_add(qconsumed);
    let apos = DNS_HEADER_LEN.saturating_add(aconsumed);
    let qtype = crate::be16_at(query, qpos);
    let atype = crate::be16_at(answer, apos);
    let qclass = crate::be16_at(query, qpos.saturating_add(2));
    let aclass = crate::be16_at(answer, apos.saturating_add(2));
    qtype.is_some() && qtype == atype && qclass.is_some() && qclass == aclass
}

/// `pack_edns0`: an 11-byte OPT record. The DO bit is bit 15 of the TTL.
pub fn pack_edns0(udp_size: u16, do_bit: bool, out: &mut [u8]) -> Option<usize> {
    let target = out.get_mut(..EDNS0_LEN)?;
    target.fill(0);
    if let Some(slot) = target.get_mut(2) {
        *slot = RR_OPT;
    }
    if let Some(slot) = target.get_mut(3) {
        *slot = u8::try_from(udp_size >> 8).unwrap_or(0);
    }
    if let Some(slot) = target.get_mut(4) {
        *slot = u8::try_from(udp_size & 0xff).unwrap_or(0);
    }
    if do_bit && let Some(slot) = target.get_mut(7) {
        *slot = 0x80;
    }
    Some(EDNS0_LEN)
}

#[cfg(test)]
#[path = "msg_ut.rs"]
mod tests;
