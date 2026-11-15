// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the indirect config protocol.

use super::*;
use crate::bdf::Bdf;
use crate::config::{ConfigSpace, identity};
use crate::sim::{BcmWindow, SimBar, SimConfig, SimDevice};

fn bdf(bus: u8, device: u8, function: u8) -> Bdf {
    Bdf::new(bus, device, function).expect("bdf")
}

#[test]
fn index_word_packs_the_address_fields() {
    assert_eq!(index_word(bdf(1, 0, 0)), 0x0010_0000);
    assert_eq!(index_word(bdf(0x80, 0x1F, 0x7)), 0x080F_F000);
    assert_eq!(index_word(bdf(0, 0, 0)), 0);
}

#[test]
fn bus_zero_addresses_the_root_header_directly() {
    assert_eq!(data_register(0, 0x10), 0x10);
    assert_eq!(data_register(0, 0xFFF), 0xFFC);
    assert_eq!(data_register(1, 0x10), EXT_CFG_DATA + 0x10);
    assert_eq!(data_register(2, 0xFFF), EXT_CFG_DATA + 0xFFC);
}

#[test]
fn reads_route_through_index_and_data() {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(bdf(1, 0, 0), 0x1DE4, 0x0001).with_class(0x0C03_3000);
    assert!(device.with_bar(0, SimBar::memory64(0x8_0000).expect("bar")));
    assert!(config.add(device));

    let mut indirect = Indirect::new(BcmWindow::new(config));
    assert_eq!(
        identity(&mut indirect, bdf(1, 0, 0)),
        Some(crate::config::Identity {
            vendor: 0x1DE4,
            device: 0x0001
        })
    );
    let (class, _) = crate::config::class(&mut indirect, bdf(1, 0, 0));
    assert_eq!(class, 0x000C_0330);
    assert_eq!(
        indirect.read32(bdf(1, 0, 0), 0x10) & 0x6,
        crate::bar::MEMORY_64
    );

    let window = indirect.release();
    assert_eq!(
        window.index(),
        index_word(bdf(1, 0, 0)),
        "the last access must leave the index at the requested function"
    );
}

#[test]
fn missing_functions_read_all_ones() {
    let mut indirect = Indirect::new(BcmWindow::new(SimConfig::new()));
    assert_eq!(indirect.read32(bdf(1, 0, 0), 0), NO_DEVICE);
}

#[test]
fn out_of_range_offsets_do_not_touch_the_window() {
    let mut indirect = Indirect::new(BcmWindow::new(SimConfig::new()));
    assert_eq!(indirect.read32(bdf(1, 0, 0), 0x1000), NO_DEVICE);
    indirect.write32(bdf(1, 0, 0), 0x1000, 0xDEAD_BEEF);
    assert_eq!(indirect.release().index(), 0);
}
