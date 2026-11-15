// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for bus/device/function addresses.

use super::*;

#[test]
fn rejects_out_of_range_fields() {
    assert_eq!(Bdf::new(0, 32, 0), Err(BdfError::Device(32)));
    assert_eq!(Bdf::new(0, 0, 8), Err(BdfError::Function(8)));
    assert!(Bdf::new(0, 31, 7).is_ok());
}

#[test]
fn route_packs_the_address() {
    let bdf = Bdf::new(1, 0, 0).expect("bdf");
    assert_eq!(bdf.route(), 0x0100);
    let bdf = Bdf::new(0x80, 0x1f, 0x7).expect("bdf");
    assert_eq!(bdf.route(), 0x80FF);
}

#[test]
fn display_is_the_conventional_form() {
    let bdf = Bdf::new(1, 2, 3).expect("bdf");
    assert_eq!(bdf.to_string(), "01:02.3");
}
