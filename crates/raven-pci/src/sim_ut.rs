// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the config-space and indirect-window simulators.

use super::*;
use crate::bdf::Bdf;
use crate::config::{ConfigSpace, identity};
use crate::indirect::{EXT_CFG_DATA, EXT_CFG_INDEX, RegisterWindow, index_word};

fn bdf(bus: u8, device: u8, function: u8) -> Bdf {
    Bdf::new(bus, device, function).expect("bdf")
}

#[test]
fn a_memory_bar_reports_its_mask_while_being_sized() {
    let mut bar = SimBar::memory32(0x1_0000).expect("bar");
    bar = bar.write(false, 0xFFFF_FFFF);
    assert_eq!(bar.read(false), 0xFFFF_0000);
    bar = bar.write(false, 0x1f00_0000);
    assert_eq!(bar.read(false), 0x1f00_0000);
    assert_eq!(bar.value(), 0x1f00_0000);
}

#[test]
fn a_64bit_bar_has_two_mask_halves() {
    let mut bar = SimBar::memory64(0x100_0000).expect("bar");
    bar = bar.write(false, 0xFFFF_FFFF);
    assert_eq!(bar.read(false), 0xFF00_0004);
    bar = bar.write(true, 0xFFFF_FFFF);
    assert_eq!(bar.read(true), 0xFFFF_FFFF);
    assert_eq!(bar.size(), 0x100_0000);
}

#[test]
fn an_unimplemented_bar_ignores_writes() {
    let bar = SimBar::unimplemented();
    assert_eq!(bar.write(false, 0xFFFF_FFFF).read(false), 0);
    assert_eq!(bar.write(false, 0x1234).read(false), 0);
}

#[test]
fn invalid_bar_sizes_are_rejected() {
    assert!(SimBar::memory32(0x8).is_none());
    assert!(SimBar::memory64(0).is_none());
    assert!(SimBar::io(0x2).is_none());
    assert!(SimBar::memory32(0x1_0000).is_some());
}

#[test]
fn the_window_routes_index_and_data() {
    let target = bdf(2, 3, 0);
    let mut config = SimConfig::new();
    assert!(config.add(SimDevice::new(target, 0x1DE4, 0x0001)));
    let mut window = BcmWindow::new(config);
    window.write32(EXT_CFG_INDEX, index_word(target));
    assert_eq!(window.index(), index_word(target));
    assert_eq!(window.read32(EXT_CFG_DATA), 0x0001_1DE4);
    assert_eq!(window.read32(EXT_CFG_DATA + 4), 0);
    assert_eq!(
        decode_index(window.index()),
        Some(target),
        "the index must decode to the selected function"
    );
}

#[test]
fn an_empty_config_space_reads_all_ones_through_the_window() {
    let mut window = BcmWindow::new(SimConfig::new());
    window.write32(EXT_CFG_INDEX, index_word(bdf(1, 0, 0)));
    assert_eq!(window.read32(EXT_CFG_DATA), NO_DEVICE);
}

#[test]
fn direct_and_missing_functions_behave_like_pci() {
    let mut config = SimConfig::new();
    assert!(config.add(SimDevice::new(bdf(1, 0, 0), 0x1DE4, 0x0001)));
    assert_eq!(
        identity(&mut config, bdf(1, 0, 0)).map(|id| id.vendor),
        Some(0x1DE4)
    );
    assert_eq!(identity(&mut config, bdf(1, 11, 0)), None);
    assert_eq!(config.read32(bdf(1, 11, 0), 0), NO_DEVICE);
}
