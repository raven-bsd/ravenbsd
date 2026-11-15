// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the controller plan.

use super::*;
use raven_pi5::parse;

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");

fn plan() -> PciePlan {
    let pi5 = parse(VALID).expect("valid Pi 5 DTB");
    let pcie = pi5.pcie.expect("PCIe node");
    let rp1 = pcie.rp1.expect("RP1 node");
    let registers = Region::new(pcie.controller.base, pcie.controller.size).expect("registers");
    let rp1 = Rp1Window::new(rp1.child_base, rp1.pci_address, rp1.cpu_base, rp1.size)
        .expect("RP1 window");
    PciePlan::new(registers, None, Some(rp1))
}

#[test]
fn the_fixture_plan_carries_the_controller_and_rp1() {
    let plan = plan();
    assert_eq!(plan.registers.base(), 0x10_0012_0000);
    assert_eq!(plan.registers.size(), 0x9310);
    assert!(plan.ecam.is_none());
    let rp1 = plan.rp1_window().expect("RP1");
    assert_eq!(rp1.cpu().base(), 0x1f_0000_0000);
    assert_eq!(rp1.pci().size(), 0x41_0000);
}

#[test]
fn a_tree_without_rp1_reports_no_window() {
    let registers = Region::new(0x10_0012_0000, 0x9310).expect("registers");
    let plan = PciePlan::new(registers, None, None);
    assert!(plan.rp1_window().is_none());
}
