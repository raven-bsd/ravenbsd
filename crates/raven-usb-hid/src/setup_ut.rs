// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the setup-packet encoder.

use super::*;

#[test]
fn class_requests_encode() {
    assert_eq!(
        SetupPacket::set_protocol(0).encode(),
        [0x21, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        SetupPacket::set_idle(0, 0).encode(),
        [0x21, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        SetupPacket::set_report(2, 0, 1).encode(),
        [0x21, 0x09, 0x00, 0x02, 0x00, 0x00, 0x01, 0x00]
    );
}

#[test]
fn get_report_descriptor_encodes() {
    assert_eq!(
        SetupPacket::get_descriptor(0x22, 0, 63).encode(),
        [0x81, 0x06, 0x00, 0x22, 0x00, 0x00, 0x3f, 0x00]
    );
}

#[test]
fn values_are_little_endian() {
    let packet = SetupPacket::new(0x21, 0x0a, 0x1234, 0x5678, 0x9abc);
    assert_eq!(
        packet.encode(),
        [0x21, 0x0a, 0x34, 0x12, 0x78, 0x56, 0xbc, 0x9a]
    );
}
