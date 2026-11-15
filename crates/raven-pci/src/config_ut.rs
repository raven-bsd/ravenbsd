// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for typed config-space access.

use super::*;
use crate::bdf::Bdf;

/// A flat 4 KiB config space with a single function.
struct Words {
    words: [u32; 0x400],
    writes: u32,
}

impl Words {
    fn new() -> Self {
        Self {
            words: [0; 0x400],
            writes: 0,
        }
    }

    fn set(&mut self, offset: u16, value: u32) {
        let index = (offset as usize / 4) & 0x3FF;
        if let Some(slot) = self.words.get_mut(index) {
            *slot = value;
        }
    }

    fn get(&self, offset: u16) -> u32 {
        let index = (offset as usize / 4) & 0x3FF;
        self.words.get(index).copied().unwrap_or(NO_DEVICE)
    }
}

impl ConfigSpace for Words {
    fn read32(&mut self, _bdf: Bdf, offset: u16) -> u32 {
        self.get(offset)
    }

    fn write32(&mut self, _bdf: Bdf, offset: u16, value: u32) {
        self.writes = self.writes.saturating_add(1);
        self.set(offset, value);
    }
}

fn bdf() -> Bdf {
    Bdf::new(1, 0, 0).expect("bdf")
}

fn device() -> Words {
    let mut words = Words::new();
    words.set(REG_VENDOR_DEVICE, 0x0001_1DE4);
    words.set(REG_REVISION_CLASS, 0x0000_0001);
    words
}

#[test]
fn all_ones_identity_reads_as_missing() {
    let mut words = Words::new();
    words.set(REG_VENDOR_DEVICE, NO_DEVICE);
    assert_eq!(identity(&mut words, bdf()), None);
}

#[test]
fn identity_decodes_vendor_and_device() {
    let mut words = device();
    assert_eq!(
        identity(&mut words, bdf()),
        Some(Identity {
            vendor: 0x1DE4,
            device: 0x0001
        })
    );
}

#[test]
fn byte_and_halfword_lanes_select_the_low_address_bits() {
    let mut words = Words::new();
    words.set(0x08, 0x1122_3344);
    assert_eq!(words.read8(bdf(), 0x08), 0x44);
    assert_eq!(words.read8(bdf(), 0x09), 0x33);
    assert_eq!(words.read8(bdf(), 0x0A), 0x22);
    assert_eq!(words.read8(bdf(), 0x0B), 0x11);
    assert_eq!(words.read16(bdf(), 0x08), 0x3344);
    assert_eq!(words.read16(bdf(), 0x0A), 0x1122);
}

#[test]
fn narrow_writes_preserve_the_other_lanes() {
    let mut words = Words::new();
    words.set(0x08, 0x1122_3344);
    words.write8(bdf(), 0x09, 0xAA);
    assert_eq!(words.get(0x08), 0x1122_AA44);
    words.write16(bdf(), 0x0A, 0xBBCC);
    assert_eq!(words.get(0x08), 0xBBCC_AA44);
}

#[test]
fn class_splits_revision_and_code() {
    let mut words = Words::new();
    words.set(REG_REVISION_CLASS, 0x0200_0001);
    let (raw, class) = class(&mut words, bdf());
    assert_eq!(raw, 0x02_0000);
    assert_eq!(
        class,
        Class {
            revision: 0x01,
            base: 0x02,
            sub: 0x00,
            interface: 0x00
        }
    );
}

#[test]
fn header_kind_and_multifunction_are_decoded() {
    let mut words = Words::new();
    words.set(REG_HEADER, 0x00_01_00_00);
    let decoded = header(&mut words, bdf());
    assert_eq!(decoded.kind, HeaderKind::Bridge);
    assert!(!decoded.multifunction);

    words.set(REG_HEADER, 0x00_80_00_00);
    let decoded = header(&mut words, bdf());
    assert_eq!(decoded.kind, HeaderKind::Endpoint);
    assert!(decoded.multifunction);

    words.set(REG_HEADER, 0x00_07_00_00);
    assert_eq!(header(&mut words, bdf()).kind, HeaderKind::Unknown(7));
}

#[test]
fn enable_sets_memory_and_bus_master() {
    let mut words = Words::new();
    words.set(REG_COMMAND_STATUS, 0x0000_0004);
    let enabled = enable_memory_and_bus_master(&mut words, bdf());
    assert_eq!(enabled & COMMAND_MEMORY_SPACE, COMMAND_MEMORY_SPACE);
    assert_eq!(enabled & COMMAND_BUS_MASTER, COMMAND_BUS_MASTER);
    assert_eq!(command(&mut words, bdf()), enabled);
}

#[test]
fn capability_walk_finds_later_entries() {
    let mut words = Words::new();
    words.set(REG_COMMAND_STATUS, u32::from(STATUS_CAPABILITIES) << 16);
    words.set(REG_CAPABILITY, 0x0000_0040);
    words.set(0x40, 0x0000_5005);
    words.set(0x50, 0x0000_0010);
    assert_eq!(find_capability(&mut words, bdf(), 0x05), Some(0x40));
    assert_eq!(
        find_capability(&mut words, bdf(), CAPABILITY_PCIE),
        Some(0x50)
    );
    assert_eq!(find_capability(&mut words, bdf(), 0x99), None);
}

#[test]
fn capability_walk_rejects_missing_and_malformed_lists() {
    let mut words = Words::new();
    words.set(REG_CAPABILITY, 0x0000_0040);
    assert_eq!(find_capability(&mut words, bdf(), CAPABILITY_PCIE), None);

    words.set(REG_COMMAND_STATUS, u32::from(STATUS_CAPABILITIES) << 16);
    words.set(REG_CAPABILITY, 0x0000_0030);
    assert_eq!(find_capability(&mut words, bdf(), CAPABILITY_PCIE), None);

    words.set(REG_CAPABILITY, 0x0000_0040);
    words.set(0x40, 0x0000_4005);
    assert_eq!(find_capability(&mut words, bdf(), CAPABILITY_PCIE), None);
}
