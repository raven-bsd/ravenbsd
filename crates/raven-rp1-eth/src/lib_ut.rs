// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions over the Pi 5 fixtures.

use super::*;
use raven_fdt_lite::Fdt;
use raven_pci::Rp1Window;
use raven_rp1::{FunctionKind, Mfd};

const MFD: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd.dtb");

#[test]
fn the_ethernet_window_matches_the_mfd_function_window() {
    let fdt = Fdt::parse(MFD).expect("fixture FDT");
    let node = EthernetNode::from_fdt(&fdt).expect("Ethernet node");
    let mfd = Mfd::from_fdt(&fdt).expect("MFD");
    let function = mfd.function(FunctionKind::Ethernet).expect("Ethernet");
    assert_eq!(function.region, node.window);
    assert!(function.enabled);
}

#[test]
fn the_ethernet_window_translates_through_the_r1_rp1_window() {
    let fdt = Fdt::parse(MFD).expect("fixture FDT");
    let node = EthernetNode::from_fdt(&fdt).expect("Ethernet node");
    let pi5 = raven_pi5::parse(MFD).expect("Pi 5 DTB");
    let pcie = pi5.pcie.expect("PCIe node");
    let rp1 = pcie.rp1.expect("RP1 node");
    let window =
        Rp1Window::new(rp1.child_base, rp1.pci_address, rp1.cpu_base, rp1.size).expect("window");
    assert_eq!(
        window.translate(node.window.base()),
        Some(0x1f_0010_0000),
        "the RP1 Ethernet window maps to this CPU address"
    );
}
