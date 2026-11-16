// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn capability_decodes_the_standard_fields() {
    let caps = Capability::from_regs(
        0x0110_0040,
        0x0000_0840,
        0x0000_0000,
        0x0000_2000,
        0x0000_3000,
    );
    assert_eq!(caps.cap_length, 0x40);
    assert_eq!(caps.version, 0x0110);
    assert_eq!(caps.max_slots, 0x40);
    assert_eq!(caps.max_interrupters, 8);
    assert_eq!(caps.max_ports, 0);
    assert_eq!(caps.operational_base(), 0x40);
    assert_eq!(caps.doorbell_base(), 0x2000);
    assert_eq!(caps.runtime_base(), 0x3000);
    assert!(!caps.ports_supported());
}

#[test]
fn capability_decodes_ports_and_scratchpad() {
    let caps = Capability::from_regs(0x0100_0020, 0x0404_0420, 0x0c10_0000, 0, 0);
    assert_eq!(caps.max_ports, 4);
    assert_eq!(caps.max_slots, 0x20);
    assert_eq!(caps.scratchpad, 1);
    assert!(caps.ports_supported());
}

#[test]
fn capability_rejects_zero_and_oversized_port_counts() {
    assert!(!Capability::from_regs(0x0100_0020, 0x0000_0020, 0, 0, 0).ports_supported());
    assert!(!Capability::from_regs(0x0100_0020, 0x0900_0020, 0, 0, 0).ports_supported());
}

#[test]
fn port_register_offsets_follow_the_stride() {
    let caps = Capability::from_regs(0x0100_0020, 0x0004_0020, 0, 0, 0);
    assert_eq!(caps.portsc(1), 0x400);
    assert_eq!(caps.portsc(4), 0x430);
    assert_eq!(caps.interrupter(0), 0);
    assert_eq!(caps.interrupter(2), 0x40);
}

#[test]
fn port_status_decodes_the_change_and_link_bits() {
    let status =
        PortStatus::from_bits(PORTSC_CCS | PORTSC_PED | PORTSC_PP | PORTSC_CSC).with_link_state(7);
    assert!(status.connected());
    assert!(status.enabled());
    assert!(status.powered());
    assert!(status.changed());
    assert_eq!(status.link_state(), 7);
    assert!(!status.with_bit(PORTSC_CSC, false).changed());
}

#[test]
fn port_status_set_and_clear_bits() {
    let status = PortStatus::from_bits(0).with_bit(PORTSC_CCS, true);
    assert!(status.connected());
    assert!(!status.with_bit(PORTSC_CCS, false).connected());
    assert_eq!(PortStatus::default().bits, 0);
}

#[test]
fn vendor_ids_are_the_vl805_function() {
    assert_eq!(XHCI_VENDOR_VIA, 0x1106);
    assert_eq!(XHCI_DEVICE_VL805, 0x3483);
}
