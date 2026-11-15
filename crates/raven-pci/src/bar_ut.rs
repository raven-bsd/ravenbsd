// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for BAR sizing, programming and assignment.

use super::*;
use crate::bdf::Bdf;
use crate::config::ConfigSpace;
use crate::sim::{SimBar, SimConfig, SimDevice};

fn bdf() -> Bdf {
    Bdf::new(1, 0, 0).expect("bdf")
}

fn single(bar: SimBar) -> SimConfig {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(bdf(), 0x1DE4, 0x0001);
    assert!(device.with_bar(0, bar));
    assert!(config.add(device));
    config
}

#[test]
fn sizes_a_32bit_memory_bar() {
    let mut config = single(SimBar::memory32(0x1_0000).expect("bar"));
    let bar = probe(&mut config, bdf(), 0).expect("probe");
    assert_eq!(bar.kind, BarKind::Memory32);
    assert_eq!(bar.size, 0x1_0000);
    assert_eq!(bar.address, 0);
    assert!(bar.is_memory());
}

#[test]
fn sizes_a_64bit_memory_bar_through_the_pair() {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(bdf(), 0x1DE4, 0x0001);
    assert!(device.with_bar(2, SimBar::memory64(0x100_0000).expect("bar")));
    assert!(config.add(device));

    let bar = probe(&mut config, bdf(), 2).expect("probe");
    assert_eq!(bar.kind, BarKind::Memory64);
    assert_eq!(bar.size, 0x100_0000);
    assert_eq!(bar.address, 0);

    let set = BarSet::read(&mut config, bdf()).expect("bars");
    assert_eq!(set.count(), 1);
    assert_eq!(set.iter().next().map(|bar| bar.index), Some(2));
}

#[test]
fn sizes_an_io_bar() {
    let mut config = single(SimBar::io(0x100).expect("bar"));
    let bar = probe(&mut config, bdf(), 0).expect("probe");
    assert_eq!(bar.kind, BarKind::Io);
    assert_eq!(bar.size, 0x100);
    assert!(!bar.is_memory());
}

#[test]
fn an_unimplemented_register_is_empty() {
    let mut config = SimConfig::new();
    assert!(config.add(SimDevice::new(bdf(), 0x1DE4, 0x0001)));
    assert_eq!(
        probe(&mut config, bdf(), 0),
        Ok(Bar {
            index: 0,
            ..Bar::NONE
        })
    );
    assert_eq!(probe(&mut config, bdf(), 6), Err(BarError::Index(6)));
}

#[test]
fn programming_verifies_and_reports_the_address() {
    let mut config = single(SimBar::memory32(0x1_0000).expect("bar"));
    let mut bar = probe(&mut config, bdf(), 0).expect("probe");
    program(&mut config, bdf(), &bar, 0x1f00_0000).expect("program");
    bar.address = 0x1f00_0000;
    let device = config.device(bdf()).expect("device");
    let model = device.bar(0).expect("bar model");
    assert_eq!(model.value(), 0x1f00_0000);
    assert_eq!(bar.region(), Region::new(0x1f00_0000, 0x1_0000));
}

#[test]
fn programming_rejects_an_invalid_alignment() {
    let mut config = single(SimBar::memory32(0x1_0000).expect("bar"));
    let bar = probe(&mut config, bdf(), 0).expect("probe");
    assert_eq!(
        program(&mut config, bdf(), &bar, 0x1f00_8000),
        Err(BarError::MisalignedBase {
            base: 0x1f00_8000,
            size: 0x1_0000
        })
    );
}

#[test]
fn programming_a_64bit_bar_uses_both_halves() {
    let mut config = SimConfig::new();
    let mut device = SimDevice::new(bdf(), 0x1DE4, 0x0001);
    assert!(device.with_bar(0, SimBar::memory64(0x100_0000).expect("bar")));
    assert!(config.add(device));
    let bar = probe(&mut config, bdf(), 0).expect("probe");
    program(&mut config, bdf(), &bar, 0x1c_0000_0000).expect("program");
    let model = config.device(bdf()).expect("device").bar(0).expect("model");
    assert_eq!(model.value(), 0x1c_0000_0004);
}

#[test]
fn programming_an_io_bar_keeps_the_type_bit() {
    let mut config = single(SimBar::io(0x100).expect("bar"));
    let bar = probe(&mut config, bdf(), 0).expect("probe");
    program(&mut config, bdf(), &bar, 0x1000).expect("program");
    let model = config.device(bdf()).expect("device").bar(0).expect("model");
    assert_eq!(model.value(), 0x1001);
}

#[test]
fn a_malformed_sizing_mask_is_rejected() {
    /// A config space whose BAR always reports a mask that is not a power-of-two complement.
    struct BadMask;
    impl ConfigSpace for BadMask {
        fn read32(&mut self, _bdf: Bdf, _offset: u16) -> u32 {
            0xFFFF_FEF0
        }
        fn write32(&mut self, _bdf: Bdf, _offset: u16, _value: u32) {}
    }
    assert_eq!(
        probe(&mut BadMask, bdf(), 0),
        Err(BarError::MalformedMask {
            index: 0,
            mask: 0xFFFF_FFFF_FFFF_FEF0
        })
    );
}

#[test]
fn allocator_hands_out_aligned_first_fit_apertures() {
    let window = Region::new(0x1f00_0000, 0x100_0000).expect("window");
    let mut allocator = WindowAllocator::new(window);
    let first = allocator.allocate(0x1_0000, 0x1_0000).expect("first");
    assert_eq!(first.base(), 0x1f00_0000);
    let second = allocator.allocate(0x40_0000, 0x40_0000).expect("second");
    assert_eq!(second.base(), 0x1f40_0000);
    assert_eq!(allocator.cursor(), 0x1f80_0000);
    let third = allocator.allocate(0x1_0000, 0x1_0000).expect("third");
    assert_eq!(third.base(), 0x1f80_0000);
    let fourth = allocator.allocate(0x1000, 0x1000).expect("fourth");
    assert_eq!(fourth.base(), 0x1f81_0000);
    assert_eq!(allocator.cursor(), 0x1f81_1000);
}

#[test]
fn allocator_rejects_invalid_requests() {
    let window = Region::new(0x1000, 0x1000).expect("window");
    let mut allocator = WindowAllocator::new(window);
    assert_eq!(allocator.allocate(0, 0x1000), None);
    assert_eq!(allocator.allocate(0x1000, 0), None);
    assert_eq!(allocator.allocate(0x1000, 0x30), None);
    assert_eq!(align_up(0x1001, 0x1000), Some(0x2000));
    assert_eq!(align_up(0x1000, 0x1000), Some(0x1000));
    assert_eq!(align_up(u64::MAX, 0x1000), None);
}

#[test]
fn assign_programs_the_bar_and_records_the_address() {
    let mut config = single(SimBar::memory32(0x1_0000).expect("bar"));
    let mut bar = probe(&mut config, bdf(), 0).expect("probe");
    let window = Region::new(0x1f00_0000, 0x10_0000).expect("window");
    let mut allocator = WindowAllocator::new(window);
    let region = allocator
        .assign(&mut config, bdf(), &mut bar)
        .expect("assign");
    assert_eq!(region.base(), 0x1f00_0000);
    assert_eq!(bar.address, 0x1f00_0000);
    assert_eq!(
        config
            .device(bdf())
            .expect("device")
            .bar(0)
            .expect("model")
            .value(),
        0x1f00_0000
    );
}

#[test]
fn a_64bit_bar_in_the_last_slot_is_rejected() {
    /// A config space that reports a valid 64-bit mask for every probe.
    struct SixtyFourInLastSlot {
        reads: u8,
    }
    impl ConfigSpace for SixtyFourInLastSlot {
        fn read32(&mut self, _bdf: Bdf, _offset: u16) -> u32 {
            self.reads = self.reads.wrapping_add(1);
            if self.reads.is_multiple_of(2) {
                0xFFFF_F004
            } else {
                0
            }
        }
        fn write32(&mut self, _bdf: Bdf, _offset: u16, _value: u32) {}
    }
    let mut config = SixtyFourInLastSlot { reads: 0 };
    assert_eq!(
        probe(&mut config, bdf(), 5),
        Err(BarError::MissingUpperHalf(5))
    );
}
