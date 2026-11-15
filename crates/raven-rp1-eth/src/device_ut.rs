// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the Ethernet device-tree node.

use super::*;
use raven_fdt_lite::Fdt;

const MFD: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd.dtb");
const MFD_MAC: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd-mac.dtb");
const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-minimal.dtb");

fn parse(blob: &[u8]) -> Result<EthernetNode, EthError> {
    EthernetNode::from_fdt(&Fdt::parse(blob).expect("fixture FDT"))
}

#[test]
fn parses_the_full_ethernet_node() {
    let node = parse(MFD).expect("Ethernet node");
    assert_eq!(node.window.base(), 0x00c0_4010_0000);
    assert_eq!(node.window.size(), 0x4000);
    assert!(node.cadence);
    assert_eq!(node.interrupt, Some(InterruptRef { id: 6, flags: 4 }));
    assert_eq!(node.clock_count, 4);
    assert_eq!(
        node.clocks[0],
        Some(ClockRef {
            id: 12,
            name: Name::new("pclk").expect("name")
        })
    );
    assert_eq!(
        node.clocks[2],
        Some(ClockRef {
            id: 29,
            name: Name::new("tsu_clk").expect("name")
        })
    );
    assert_eq!(node.phy_mode, Some(PhyMode::RgmiiId));
    assert_eq!(node.mac, None, "the firmware placeholder is all zero");
    assert_eq!(node.aw2w_max_pipe, Some(8));
    assert_eq!(node.ar2r_max_pipe, Some(8));
    assert!(node.use_aw2b_fill);
    assert_eq!(
        node.reset,
        Some(ResetGpio {
            pin: 32,
            active_low: true
        })
    );
    assert_eq!(node.reset_duration_ms, Some(5));
    assert_eq!(
        node.phy,
        Some(PhyRef {
            address: 1,
            powerdown: true
        })
    );
}

#[test]
fn a_real_mac_address_is_kept() {
    let node = parse(MFD_MAC).expect("Ethernet node");
    let address = node.mac.expect("non-zero local MAC");
    assert_eq!(address.to_string(), "02:11:22:33:44:55");
    assert!(address.is_unicast());
}

#[test]
fn the_minimal_node_has_only_the_window() {
    let node = parse(VALID).expect("Ethernet node");
    assert_eq!(node.window.base(), 0x00c0_4010_0000);
    assert_eq!(node.window.size(), 0x4000);
    assert!(!node.cadence);
    assert_eq!(node.interrupt, None);
    assert_eq!(node.clock_count, 0);
    assert_eq!(node.phy_mode, None);
    assert_eq!(node.mac, None);
    assert_eq!(node.aw2w_max_pipe, None);
    assert_eq!(node.ar2r_max_pipe, None);
    assert!(!node.use_aw2b_fill);
    assert_eq!(node.reset, None);
    assert_eq!(node.reset_duration_ms, None);
    assert_eq!(node.phy, None);
}

#[test]
fn a_tree_without_the_node_is_rejected() {
    assert_eq!(parse(MINIMAL), Err(EthError::MissingNode));
}

#[test]
fn the_phy_mode_names_are_mapped() {
    assert_eq!(PhyMode::parse("rgmii"), PhyMode::Rgmii);
    assert_eq!(PhyMode::parse("rgmii-id"), PhyMode::RgmiiId);
    assert_eq!(PhyMode::parse("rgmii-txid"), PhyMode::RgmiiTxId);
    assert_eq!(PhyMode::parse("rgmii-rxid"), PhyMode::RgmiiRxId);
    assert_eq!(PhyMode::parse("mii"), PhyMode::Unknown);
}
