// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the ECAM window.

use super::*;

fn bdf(bus: u8, device: u8, function: u8) -> Bdf {
    Bdf::new(bus, device, function).expect("bdf")
}

#[test]
fn rejects_invalid_windows() {
    assert_eq!(Ecam::new(0, 0, 0, 0), Err(EcamError::Empty));
    assert_eq!(
        Ecam::new(0, 0x10_0000, 2, 1),
        Err(EcamError::BusOrder { first: 2, last: 1 })
    );
    assert_eq!(
        Ecam::new(0, 0x10_0000, 0, 1),
        Err(EcamError::TooSmall {
            buses: 2,
            length: 0x10_0000
        })
    );
    assert_eq!(
        Ecam::new(0, 0x10_0000 + 0x800, 0, 0),
        Err(EcamError::UnalignedLength(0x10_0800))
    );
    assert_eq!(
        Ecam::new(0x800, 0x10_0000, 0, 0),
        Err(EcamError::Misaligned { value: 0x800 })
    );
    assert_eq!(
        Ecam::new(u64::MAX - 0xFFF, 0x10_0000, 0, 0),
        Err(EcamError::Overflow)
    );
}

#[test]
fn offsets_follow_the_ecam_layout() {
    let window = Ecam::new(0x30_0000_0000, 2 * BUS_SPAN, 1, 2).expect("ecam");
    assert_eq!(window.offset(bdf(1, 0, 0), 0), Some(0));
    assert_eq!(window.offset(bdf(1, 0, 0), 0x10), Some(0x10));
    assert_eq!(window.offset(bdf(1, 0, 1), 0), Some(0x1000));
    assert_eq!(window.offset(bdf(1, 1, 0), 0), Some(0x8000));
    assert_eq!(window.offset(bdf(2, 0, 0), 0x3C), Some(BUS_SPAN + 0x3C));
    assert_eq!(window.address(bdf(1, 0, 0), 0), Some(0x30_0000_0000));
}

#[test]
fn rejects_addresses_outside_the_window() {
    let window = Ecam::new(0x30_0000_0000, BUS_SPAN, 1, 1).expect("ecam");
    assert_eq!(window.offset(bdf(0, 0, 0), 0), None);
    assert_eq!(window.offset(bdf(2, 0, 0), 0), None);
    assert_eq!(window.offset(bdf(1, 0, 0), 0x1000), None);
    assert_eq!(window.address(bdf(0, 0, 0), 0), None);
}
