// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the xHCI register decoding.

use super::*;

#[test]
fn capability_fields_are_decoded() {
    let hcsparams1 = 32 | (1 << 8) | (2 << 24);
    let hccparams1 = 1 | (1 << 3) | (0x0100 << 16);
    let capability = Capability::decode(0x0110, hcsparams1, hccparams1);
    assert_eq!(capability.hci_version, 0x0110);
    assert_eq!(capability.max_slots, 32);
    assert_eq!(capability.max_interrupters, 1);
    assert_eq!(capability.max_ports, 2);
    assert!(capability.ac64);
    assert!(!capability.csz);
    assert!(capability.ppc);
    assert!(!capability.pind);
    assert_eq!(capability.max_psa_size, 0);
    assert_eq!(capability.xecp, 0x0100);
}

#[test]
fn hcsparams2_fields_are_decoded() {
    let parameters = decode_hcsparams2(0x43);
    assert_eq!(parameters.ist, 3);
    assert_eq!(parameters.erst_max, 4);
}

#[test]
fn reserved_offset_bits_are_cleared() {
    assert_eq!(Capability::operational_offset(0x22), 0x20);
    assert_eq!(Capability::runtime_offset(0x1234), 0x1220);
    assert_eq!(Capability::doorbell_offset(0x2005), 0x2004);
}

#[test]
fn portsc_round_trips() {
    let value = PORTSC_CCS | PORTSC_PED | PORTSC_PP | PORTSC_CSC | PORTSC_PRC;
    let status = PortStatus::decode(value);
    assert!(status.connected);
    assert!(status.enabled);
    assert!(status.powered);
    assert!(status.connect_change);
    assert!(status.reset_change);
    assert!(!status.overcurrent);
    assert_eq!(status.link_state, 0);
    assert_eq!(status.encode(), value);
}

#[test]
fn portsc_link_state_is_a_four_bit_field() {
    let status = PortStatus {
        connected: true,
        enabled: false,
        overcurrent: false,
        reset: false,
        link_state: 7,
        powered: true,
        connect_change: false,
        enable_change: false,
        reset_change: false,
        link_change: true,
        overcurrent_change: false,
    };
    let encoded = status.encode();
    assert_eq!((encoded >> PORTSC_PLS_SHIFT) & PORTSC_PLS_MASK, 7);
    assert_eq!(PortStatus::decode(encoded), status);
}

#[test]
fn the_standard_register_bits_are_stable() {
    assert_eq!(USBCMD_RUN, 0x0000_0001);
    assert_eq!(USBCMD_RESET, 0x0000_0002);
    assert_eq!(USBSTS_HCH, 0x0000_0001);
    assert_eq!(USBSTS_CNR, 0x0000_0800);
    assert_eq!(PORT_BASE, 0x400);
    assert_eq!(PORT_STRIDE, 0x10);
    assert_eq!(ERDP_EHB, 8);
    assert_eq!(IMAN_IP, 1);
}
