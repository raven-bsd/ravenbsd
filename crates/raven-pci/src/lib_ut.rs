// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions over the B54 Pi 5 fixture.

use super::*;
use crate::bar::program;
use raven_pi5::parse;

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");

fn fixture() -> (PciePlan, raven_pi5::Pcie) {
    let pi5 = parse(VALID).expect("valid Pi 5 DTB");
    let pcie = pi5.pcie.expect("PCIe node");
    let rp1 = pcie.rp1.clone().expect("RP1 node");
    let registers = Region::new(pcie.controller.base, pcie.controller.size).expect("registers");
    let window = Rp1Window::new(rp1.child_base, rp1.pci_address, rp1.cpu_base, rp1.size)
        .expect("RP1 window");
    (PciePlan::new(registers, None, Some(window)), pcie)
}

#[test]
fn the_fixture_tree_declares_no_ecam_window() {
    // The B54 fixture is transcribed from the official Pi 5 firmware DTB, which has a 4 KiB
    // register block and no ECAM range. The seam therefore uses the indirect registers.
    let (plan, _) = fixture();
    assert!(plan.ecam.is_none());
    assert_eq!(plan.registers.size(), 0x9310);
}

#[test]
fn the_rp1_window_lies_inside_the_pcie_32bit_range() {
    let (plan, pcie) = fixture();
    let window = plan.rp1_window().expect("RP1");
    let mut covered = false;
    for range in &pcie.ranges {
        if let Some(region) = Region::new(range.pci, range.size)
            && region.contains_region(window.pci())
        {
            covered = true;
        }
    }
    assert!(covered, "the RP1 window must fit a PCIe ranges entry");
}

#[test]
fn an_unprogrammed_endpoint_is_still_identified_and_can_be_assigned() {
    let (plan, _) = fixture();
    let window = plan.rp1_window().expect("RP1");
    let bdf = Bdf::new(1, 0, 0).expect("bdf");

    let mut config = sim::SimConfig::new();
    let mut endpoint = sim::SimDevice::new(bdf, 0x1DE4, 0x0001);
    assert!(endpoint.with_bar(0, sim::SimBar::memory64(0x0080_0000).expect("BAR")));
    assert!(config.add(endpoint));

    let found = discover(&mut config, Scan::single(1), window).expect("RP1");
    assert_eq!(found.bar.address, 0, "the firmware left the BAR unassigned");
    assert_eq!(found.bar.size, 0x0080_0000);

    let command = enable_memory_and_bus_master(&mut config, bdf);
    assert_eq!(
        command & config::COMMAND_MEMORY_SPACE,
        config::COMMAND_MEMORY_SPACE
    );
    assert_eq!(
        command & config::COMMAND_BUS_MASTER,
        config::COMMAND_BUS_MASTER
    );

    let bar = found.bar;
    program(&mut config, bdf, &bar, window.pci().base()).expect("assign the RP1 BAR");
    assert_eq!(bar.address, window.pci().base());
    let rediscovered = discover(&mut config, Scan::single(1), window).expect("RP1 again");
    assert_eq!(rediscovered.bar.address, window.pci().base());
}

#[test]
fn an_ecam_window_rejects_the_root_complex_register_span() {
    // The controller register block is 0x9310 bytes, not a whole-bus ECAM span; using it as an
    // ECAM window must fail before any address is computed.
    assert!(Ecam::new(0x10_0012_0000, 0x9310, 0, 0).is_err());
}
