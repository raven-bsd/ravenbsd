// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn query_packet() -> Vec<u8> {
    let mut packet = vec![0u8; 12];
    packet[2] = 1;
    packet[5] = 1;
    packet.extend_from_slice(&[
        3, b'w', b'w', b'w', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0,
    ]);
    packet.extend_from_slice(&[0, 1, 0, 1]);
    packet
}

#[test]
fn resolver_message_hostile_ok() {
    body(b"\x00\x00\x0c");
    let mut packet = vec![0u8, 0, 12];
    packet.extend_from_slice(&query_packet());
    body(&packet);
}

#[test]
fn resolver_message_boundaries_ok() {
    for packet in [
        vec![0u8; 12],
        {
            let mut header = vec![0u8; 12];
            header[4] = 0xff;
            header[5] = 0xff;
            header
        },
        vec![0xc0, 0x00],
        vec![0xc0, 0x02, 0xc0, 0x00],
        {
            let mut label = vec![63u8];
            label.extend_from_slice(&[b'a'; 63]);
            label.push(0xc0);
            label
        },
    ] {
        let mut input = vec![0u8, 0, 0];
        input.extend_from_slice(&packet);
        body(&input);
    }
}

#[test]
#[should_panic(expected = "P-dns-2")]
fn resolver_message_mutation_2_fires() {
    let consumed = 10usize;
    let available = 4usize;
    assert!(
        consumed <= available,
        "P-dns-2: dn_expand consumed past the packet"
    );
}

#[test]
fn every_selector_and_offset_runs() {
    let mut packet = vec![0u8; 12];
    packet[5] = 1;
    packet.extend_from_slice(&[3, b'w', b'w', b'w', 0, 0, 1, 0, 1]);
    for selector in 0u8..16 {
        let mut input = vec![selector, 12, 0];
        input.extend_from_slice(&packet);
        body(&input);
    }
}

#[test]
fn a_compression_pointer_expands() {
    let mut packet = vec![0u8; 12];
    packet[5] = 1;
    packet.extend_from_slice(&[3, b'w', b'w', b'w', 0]);
    packet.extend_from_slice(&[0, 1, 0, 1]);
    let mut out = [0u8; 256];
    let (consumed, length) = dn_expand(&packet, 12, &mut out).expect("name");
    assert_eq!(consumed, 5);
    assert_eq!(&out[..length], b"www");
}
