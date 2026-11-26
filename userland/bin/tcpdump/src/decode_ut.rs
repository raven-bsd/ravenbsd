// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the packet printer.

use super::*;
use crate::pcap::TimestampPrecision;

fn arp_request() -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&[0xff; 6]);
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 0x0a]);
    frame.extend_from_slice(&0x0806u16.to_be_bytes());
    frame.extend_from_slice(&1u16.to_be_bytes());
    frame.extend_from_slice(&0x0800u16.to_be_bytes());
    frame.push(6);
    frame.push(4);
    frame.extend_from_slice(&1u16.to_be_bytes());
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 0x0a]);
    frame.extend_from_slice(&[10, 0, 0, 5]);
    frame.extend_from_slice(&[0u8; 6]);
    frame.extend_from_slice(&[192, 0, 2, 10]);
    frame
}

fn packet(data: &[u8]) -> Packet<'_> {
    Packet {
        ts_sec: 1_700_000_000,
        ts_frac: 0,
        precision: TimestampPrecision::Microseconds,
        caplen: data.len() as u32,
        orig_len: data.len() as u32,
        data,
    }
}

#[test]
fn formats_ipv6_with_the_longest_zero_run() {
    let mut address = [0u8; 16];
    address[0] = 0x20;
    address[1] = 0x01;
    address[2] = 0x0d;
    address[3] = 0xb8;
    address[15] = 5;
    assert_eq!(format_ipv6(&address), "2001:db8::5");
    assert_eq!(format_ipv6(&[0u8; 16]), "::");
    assert_eq!(
        format_ipv6(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        "::1"
    );
}

#[test]
fn formats_tcp_flags() {
    assert_eq!(format_tcp_flags(0x02), "S");
    assert_eq!(format_tcp_flags(0x12), "S.");
    assert_eq!(format_tcp_flags(0x18), "P.");
    assert_eq!(format_tcp_flags(0x11), "F.");
    assert_eq!(format_tcp_flags(0x10), ".");
    assert_eq!(format_tcp_flags(0), "none");
}

#[test]
fn decodes_an_arp_request() {
    let frame = arp_request();
    let mut decoder = Decoder::new();
    let lines = decoder.decode(&packet(&frame), &DecodeOptions::default());
    assert_eq!(
        lines,
        vec![String::from(
            "ARP, Request who-has 192.0.2.10 tell 10.0.0.5, length 28"
        )]
    );
}

#[test]
fn prints_the_unix_timestamp_prefix() {
    let frame = arp_request();
    let mut decoder = Decoder::new();
    let options = DecodeOptions {
        timestamps: Timestamps::Unix,
        ..DecodeOptions::default()
    };
    let lines = decoder.decode(&packet(&frame), &options);
    assert!(
        lines[0].starts_with("1700000000.000000 ARP, Request"),
        "{lines:?}"
    );
}
