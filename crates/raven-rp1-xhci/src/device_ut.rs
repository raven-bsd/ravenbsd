// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the RP1 USB device-tree nodes.

use super::*;
use raven_fdt_lite::Fdt;

const XHCI: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-xhci.dtb");
const MFD: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-mfd.dtb");
const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");

fn parse(blob: &[u8]) -> Result<UsbControllers, UsbError> {
    UsbControllers::from_fdt(&Fdt::parse(blob).expect("fixture FDT"))
}

#[test]
fn parses_both_full_controllers() {
    let controllers = parse(XHCI).expect("USB nodes");
    assert_eq!(controllers.count(), 2);
    assert_eq!(controllers.enabled_count(), 2);

    let first = controllers.get(0).expect("controller 0");
    assert_eq!(first.index, 0);
    assert_eq!(first.window.base(), 0x00c0_4020_0000);
    assert_eq!(first.window.size(), 0x10_0000);
    assert_eq!(first.interrupt, Some(InterruptRef { id: 0x1F, flags: 1 }));
    assert_eq!(first.dr_mode, Some(DrMode::Host));
    assert!(first.lpm);
    assert_eq!(first.axi_pipe_limit, Some(8));
    assert_eq!(first.tx_max_burst, Some(8));
    assert_eq!(first.tx_thr_num_pkt, Some(2));
    assert!(first.quirks.dis_rxdet_inp3);
    assert!(first.quirks.enhanced_nak_fs);
    assert!(first.quirks.parkmode_disable_ss);
    assert!(first.quirks.parkmode_disable_fsls);
    assert!(first.enabled);

    let second = controllers.get(1).expect("controller 1");
    assert_eq!(second.window.base(), 0x00c0_4030_0000);
    assert_eq!(second.interrupt, Some(InterruptRef { id: 0x24, flags: 1 }));
}

#[test]
fn the_mfd_variant_has_disabled_minimal_nodes() {
    let controllers = parse(MFD).expect("USB nodes");
    assert_eq!(controllers.count(), 2);
    assert_eq!(controllers.enabled_count(), 0);
    let first = controllers.get(0).expect("controller 0");
    assert_eq!(first.window.base(), 0x00c0_4020_0000);
    assert_eq!(first.interrupt, None);
    assert_eq!(first.dr_mode, None);
    assert!(!first.lpm);
    assert_eq!(first.axi_pipe_limit, None);
    assert!(!first.enabled);
}

#[test]
fn a_tree_without_usb_nodes_is_rejected() {
    assert_eq!(parse(VALID), Err(UsbError::MissingNode));
}

#[test]
fn the_dr_mode_names_are_mapped() {
    assert_eq!(DrMode::parse("host"), DrMode::Host);
    assert_eq!(DrMode::parse("peripheral"), DrMode::Peripheral);
    assert_eq!(DrMode::parse("otg"), DrMode::Otg);
    assert_eq!(DrMode::parse("mystery"), DrMode::Unknown);
}
