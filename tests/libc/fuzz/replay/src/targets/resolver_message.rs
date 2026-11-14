// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 9: the DNS message codec (C51).
//!
//! The body parses arbitrary packets, expands a name at a chosen offset, round-trips a name
//! through `dn_comp`/`dn_expand`, checks `question_matches` and builds a query.

use raven_libc_resolv::{dn_comp, dn_expand, dns_parse, question_matches, res_mkquery};

use crate::model;
use crate::{Cursor, alloc_probe};

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 9.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let selector = cursor.u8();
    let offset = usize::from(cursor.u16());
    let packet = cursor.take(1500);

    let mut records = 0usize;
    let packet_start = packet.as_ptr() as usize;
    let packet_end = packet_start.saturating_add(packet.len());
    let mut outside = false;
    let result = dns_parse(packet, |_rr, rdata, full| {
        records = records.saturating_add(1);
        let start = rdata.as_ptr() as usize;
        let end = start.saturating_add(rdata.len());
        if start < full.as_ptr() as usize
            || end > (full.as_ptr() as usize).saturating_add(full.len())
            || start < packet_start
            || end > packet_end
        {
            outside = true;
        }
        0
    });
    assert!(!outside, "P-dns-4: callback rdata lies outside the packet");
    let _ = result;

    let mut name = [0u8; 256];
    if let Some((consumed, length)) = dn_expand(packet, offset, &mut name) {
        assert!(
            offset <= packet.len(),
            "P-dns-2: dn_expand started past the packet"
        );
        assert!(
            consumed <= packet.len().saturating_sub(offset),
            "P-dns-2: dn_expand consumed past the packet"
        );
        assert!(length <= 253, "P-dns-2: dn_expand produced a long name");
    }

    let derived: Vec<u8> = packet
        .iter()
        .take(63)
        .copied()
        .filter(|byte| byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'.')
        .collect();
    let name = if derived.is_empty() {
        b"example".to_vec()
    } else {
        derived
    };
    let mut compressed = [0u8; 512];
    let mut offsets = [0usize; 8];
    if let Some(length) = dn_comp(&name, &mut compressed, &mut offsets, None) {
        assert!(length <= compressed.len(), "P-dns-5: dn_comp overran");
        let mut expanded = [0u8; 256];
        if let Some((_, text_length)) = dn_expand(&compressed, 0, &mut expanded) {
            let text = expanded.get(..text_length).unwrap_or(&[]);
            assert_eq!(
                model::dn(text),
                model::dn(&name),
                "P-dns-5: dn_comp then dn_expand changed the name"
            );
        }
    }

    let _ = question_matches(packet, packet);
    if !packet.is_empty() {
        let mut mutated = packet.to_vec();
        if let Some(slot) = mutated.get_mut(packet.len() / 2) {
            *slot ^= 0x80;
        }
        let _ = question_matches(packet, &mutated);
    }

    let mut query = [0u8; 512];
    if let Some(length) = res_mkquery(selector & 15, &name, 1, 1, u16::from(selector), &mut query) {
        assert!(length <= 512, "P-dns-6: res_mkquery exceeded 512 bytes");
    }

    alloc_probe::note(hash(&[
        records as u64,
        u64::from(selector),
        offset as u64,
        u64::from(name.len() as u32),
    ]));
}

#[cfg(test)]
#[path = "resolver_message_ut.rs"]
mod tests;
