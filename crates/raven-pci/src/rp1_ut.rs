// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for RP1 discovery.

use super::*;
use crate::bdf::Bdf;
use crate::config::ConfigSpace;
use crate::sim::{SimBar, SimConfig, SimDevice};
use raven_pi5::parse;

const VALID: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-valid.dtb");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-minimal.dtb");

/// The RP1 window exactly as the B54 fixture describes it.
fn fixture_window() -> Rp1Window {
    let pi5 = parse(VALID).expect("valid Pi 5 DTB");
    let pcie = pi5.pcie.expect("PCIe node");
    let rp1 = pcie.rp1.expect("RP1 node");
    Rp1Window::new(rp1.child_base, rp1.pci_address, rp1.cpu_base, rp1.size).expect("window")
}

#[test]
fn the_fixture_window_carries_all_three_views() {
    let window = fixture_window();
    assert_eq!(window.local().base(), 0xc0_4000_0000);
    assert_eq!(window.pci().base(), 0);
    assert_eq!(window.cpu().base(), 0x1f_0000_0000);
    assert_eq!(window.pci().size(), 0x41_0000);
    assert_eq!(window.translate(0xc0_4003_0000), Some(0x1f_0003_0000));
    assert_eq!(window.translate(0x1f_0003_0000), None);
}

#[test]
fn discovers_the_endpoint_whose_bar_covers_the_window() {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(Bdf::new(1, 0, 0).expect("bdf"), 0x1DE4, 0x0001);
    assert!(device.with_bar(0, SimBar::memory64(0x0080_0000).expect("bar")));
    assert!(config.add(device));

    let device = discover(&mut config, Scan::single(1), fixture_window()).expect("RP1");
    assert_eq!(device.bdf, Bdf::new(1, 0, 0).expect("bdf"));
    assert_eq!(device.identity.vendor, 0x1DE4);
    assert_eq!(device.bar.kind, crate::bar::BarKind::Memory64);
    assert_eq!(device.bar.size, 0x0080_0000);
    assert!(
        device
            .aperture()
            .is_some_and(|aperture| aperture.contains_region(fixture_window().pci()))
    );
}

#[test]
fn a_window_larger_than_the_bar_is_not_discovered() {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(Bdf::new(1, 0, 0).expect("bdf"), 0x1DE4, 0x0001);
    assert!(device.with_bar(0, SimBar::memory32(0x1_0000).expect("bar")));
    assert!(config.add(device));
    assert_eq!(
        discover(&mut config, Scan::single(1), fixture_window()),
        Err(DiscoverError::NoDevice)
    );
}

#[test]
fn a_device_on_another_bus_is_not_found_on_bus_one() {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(Bdf::new(2, 0, 0).expect("bdf"), 0x1DE4, 0x0001);
    assert!(device.with_bar(0, SimBar::memory64(0x0080_0000).expect("bar")));
    assert!(config.add(device));

    assert_eq!(
        discover(&mut config, Scan::single(1), fixture_window()),
        Err(DiscoverError::NoDevice)
    );
    let found = discover(
        &mut config,
        Scan::buses(1, 2).expect("scan"),
        fixture_window(),
    )
    .expect("RP1");
    assert_eq!(found.bdf.bus(), 2);
}

#[test]
fn two_matching_functions_are_ambiguous() {
    let mut config = SimConfig::new();
    for device_number in 0..2u8 {
        let mut device =
            SimDevice::new(Bdf::new(1, device_number, 0).expect("bdf"), 0x1DE4, 0x0001);
        assert!(device.with_bar(0, SimBar::memory64(0x0080_0000).expect("bar")));
        assert!(config.add(device));
    }
    assert_eq!(
        discover(&mut config, Scan::single(1), fixture_window()),
        Err(DiscoverError::Ambiguous {
            first: Bdf::new(1, 0, 0).expect("bdf"),
            second: Bdf::new(1, 1, 0).expect("bdf"),
        })
    );
}

#[test]
fn a_malformed_device_does_not_stop_the_scan() {
    /// A config space that corrupts the BAR mask of device 0 on bus 1.
    struct Corrupt {
        inner: SimConfig,
    }
    impl ConfigSpace for Corrupt {
        fn read32(&mut self, bdf: Bdf, offset: u16) -> u32 {
            if bdf.device() == 0 && offset == crate::config::REG_BAR0 {
                return 0xFFFF_FEF0;
            }
            self.inner.read32(bdf, offset)
        }
        fn write32(&mut self, bdf: Bdf, offset: u16, value: u32) {
            self.inner.write32(bdf, offset, value);
        }
    }

    let mut inner = SimConfig::new();
    let mut broken = SimDevice::new(Bdf::new(1, 0, 0).expect("bdf"), 0x1234, 0x5678);
    assert!(broken.with_bar(0, SimBar::memory64(0x0080_0000).expect("bar")));
    assert!(inner.add(broken));
    let mut good = SimDevice::new(Bdf::new(1, 1, 0).expect("bdf"), 0x1DE4, 0x0001);
    assert!(good.with_bar(0, SimBar::memory64(0x0080_0000).expect("bar")));
    assert!(inner.add(good));

    let mut config = Corrupt { inner };
    let found = discover(&mut config, Scan::single(1), fixture_window()).expect("RP1");
    assert_eq!(found.bdf.device(), 1);
}

#[test]
fn a_tree_without_pcie_has_no_rp1_window() {
    let pi5 = parse(MINIMAL).expect("minimal Pi 5 DTB");
    assert!(pi5.pcie.is_none());
}

#[test]
fn scan_ranges_are_validated() {
    assert!(Scan::buses(2, 1).is_none());
    assert_eq!(Scan::all().first_bus(), 0);
    assert_eq!(Scan::all().last_bus(), 0xFF);
    assert_eq!(Scan::single(1).first_bus(), 1);
}
