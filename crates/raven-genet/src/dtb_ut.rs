// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the Pi 4 GENET device-tree parser.

use super::*;
use crate::mac::MacAddress;
use crate::phy::PhyMode;
use raven_fdt_lite::DmaWindow;

const VALID_HEX: &str = include_str!("../../../tests/fixtures/pi4-genet/pi4-genet-valid.dtb.hex");
const NO_MAC_HEX: &str = include_str!("../../../tests/fixtures/pi4-genet/pi4-genet-no-mac.dtb.hex");
const BAD_MDIO_HEX: &str =
    include_str!("../../../tests/fixtures/pi4-genet/pi4-genet-bad-mdio.dtb.hex");

fn decode_hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .map(|byte| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("fixture contains a non-hex byte"),
        })
        .collect();
    assert!(digits.len().is_multiple_of(2));
    digits
        .chunks(2)
        .map(|pair| (pair[0] << 4) | pair[1])
        .collect()
}

fn valid() -> Vec<u8> {
    decode_hex(VALID_HEX)
}

fn no_mac() -> Vec<u8> {
    decode_hex(NO_MAC_HEX)
}

fn bad_mdio() -> Vec<u8> {
    decode_hex(BAD_MDIO_HEX)
}

#[test]
fn the_valid_fixture_has_the_pi4_register_window() {
    let info = parse(&valid()).expect("valid fixture");
    assert_eq!(info.base, 0xfd58_0000);
    assert_eq!(info.size, 0x1_0000);
    assert_eq!(info.mdio_offset, 0xe14);
    assert_eq!(info.mdio_size, 8);
    assert_eq!(info.mdio_base(), Some(0xfd58_0e14));
}

#[test]
fn the_valid_fixture_has_both_gic_spis() {
    let info = parse(&valid()).expect("valid fixture");
    assert_eq!(info.interrupts, [157, 158]);
    assert_eq!(info.interrupt_flags, [4, 4]);
}

#[test]
fn the_valid_fixture_selects_phy_one_and_rgmii_rxid() {
    let info = parse(&valid()).expect("valid fixture");
    assert_eq!(info.phy_address, 1);
    assert_eq!(info.phy_mode, PhyMode::RgmiiRxid);
}

#[test]
fn the_valid_fixture_carries_the_synthetic_mac() {
    let info = parse(&valid()).expect("valid fixture");
    assert_eq!(info.mac, Some(MacAddress::new([0x02, 0, 0, 0, 0, 0x42])));
}

#[test]
fn the_valid_fixture_has_the_two_scb_dma_windows() {
    let info = parse(&valid()).expect("valid fixture");
    assert!(!info.dma.is_identity());
    let windows = info.dma.as_slice();
    assert_eq!(windows.len(), 2);
    assert_eq!(
        windows[0],
        DmaWindow {
            bus_base: 0x4_7c00_0000,
            cpu_base: 0xfc00_0000,
            size: 0x0380_0000,
        }
    );
    assert_eq!(
        windows[1],
        DmaWindow {
            bus_base: 0,
            cpu_base: 0,
            size: 0x4_0000_0000,
        }
    );
}

#[test]
fn the_dma_windows_translate_in_both_directions() {
    let info = parse(&valid()).expect("valid fixture");
    assert_eq!(info.dma.translate_bus(0), Some(0));
    assert_eq!(info.dma.translate_bus(0x4_7c00_1000), Some(0xfc00_1000));
    assert_eq!(info.dma.translate_cpu(0xfc00_1000), Some(0x4_7c00_1000));
    assert_eq!(info.dma.translate_bus(0x4_7c00_0000 + 0x0380_0000), None);
}

#[test]
fn a_tree_without_a_mac_property_yields_no_address() {
    let info = parse(&no_mac()).expect("no-mac fixture");
    assert_eq!(info.mac, None);
    assert_eq!(info.phy_mode, PhyMode::RgmiiId);
}

#[test]
fn a_malformed_mdio_register_is_rejected() {
    assert_eq!(parse(&bad_mdio()), Err(DtbError::BadMdio));
}

#[test]
fn a_bad_magic_is_reported_as_an_fdt_error() {
    let mut data = valid();
    data[0] = 0;
    assert!(matches!(parse(&data), Err(DtbError::Fdt(_))));
}

#[test]
fn a_truncated_blob_is_rejected_without_panicking() {
    let data = valid();
    assert!(matches!(
        parse(&data[..20]),
        Err(DtbError::Fdt(FdtError::TooShort))
    ));
}

#[test]
fn the_fixture_data_is_a_supported_fdt_version() {
    let data = valid();
    assert_eq!(
        u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
        17
    );
}

#[test]
fn an_empty_window_list_is_an_identity_map() {
    let map = DmaMap::from_windows(&[]);
    assert!(map.is_identity());
    assert!(map.as_slice().is_empty());
    assert_eq!(map.translate_bus(0x1234_5678), Some(0x1234_5678));
    assert_eq!(map.translate_cpu(0x1234_5678), Some(0x1234_5678));
}

#[test]
fn the_default_dma_map_is_identity() {
    let map = DmaMap::default();
    assert!(map.is_identity());
    assert_eq!(map.translate_bus(0xdead_beef), Some(0xdead_beef));
}

#[test]
fn more_windows_than_the_fixed_array_are_truncated() {
    let window = DmaWindow {
        bus_base: 0,
        cpu_base: 0,
        size: 1,
    };
    let map = DmaMap::from_windows(&[window; MAX_DMA_WINDOWS + 1]);
    assert_eq!(map.as_slice().len(), MAX_DMA_WINDOWS);
}
