// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The misc group surface.

use super::*;

#[test]
fn reexports_are_live() {
    assert_eq!(a64l(b"."), Ok(0));
    let mut out = [0u8; 8];
    l64a(1, &mut out);
    assert_eq!(out[0], b'/');
    let mut options = b"x\0".to_vec();
    assert_eq!(getsubopt(0, &mut options, &[b"x"]).index, Some(0));
}

#[test]
fn subopt_shape_is_comparable() {
    assert_eq!(
        Subopt {
            index: None,
            token: None,
            value: None,
            next: 0
        },
        Subopt {
            index: None,
            token: None,
            value: None,
            next: 0
        }
    );
}

#[test]
fn a64l_and_l64a_are_inverse_for_small_values() {
    let mut out = [0u8; 8];
    for value in 0..1000i64 {
        l64a(value, &mut out);
        let length = out.iter().position(|byte| *byte == 0).unwrap_or(8);
        assert_eq!(a64l(&out[..length]), Ok(value));
    }
}

#[test]
fn getsubopt_reports_a_value() {
    let mut options = b"opt=value\0".to_vec();
    let result = getsubopt(0, &mut options, &[b"opt"]);
    assert_eq!(result.index, Some(0));
    assert_eq!(result.value, Some((4, 5)));
}
