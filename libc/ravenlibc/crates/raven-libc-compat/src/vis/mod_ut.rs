// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Module-level `vis` tests: the installed-header parity is checked in `tests/header_parity.rs`;
//! here the exported names and constant values are asserted.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

#[test]
fn flag_values_match_the_netbsd_header() {
    assert_eq!(VIS_OCTAL, 0x0001);
    assert_eq!(VIS_CSTYLE, 0x0002);
    assert_eq!(VIS_SP, 0x0004);
    assert_eq!(VIS_TAB, 0x0008);
    assert_eq!(VIS_NL, 0x0010);

    assert_eq!(VIS_WHITE, 0x001c);
    assert_eq!(VIS_SAFE, 0x0020);
    assert_eq!(VIS_NOSLASH, 0x0040);
    assert_eq!(VIS_HTTP1808, 0x0080);
    assert_eq!(VIS_HTTPSTYLE, 0x0080);
    assert_eq!(VIS_MIMESTYLE, 0x0100);
    assert_eq!(VIS_HTTP1866, 0x0200);
    assert_eq!(VIS_NOESCAPE, 0x0400);
    assert_eq!(VIS_END, 0x0800);
    assert_eq!(VIS_GLOB, 0x1000);
    assert_eq!(VIS_SHELL, 0x2000);
    assert_eq!(VIS_META, 0x301c);
    assert_eq!(VIS_NOLOCALE, 0x4000);
    assert_eq!(VIS_DQ, 0x8000);
}

#[test]
fn unvis_result_values_match_the_header() {
    assert_eq!(UNVIS_VALID, 1);
    assert_eq!(UNVIS_VALIDPUSH, 2);
    assert_eq!(UNVIS_NOCHAR, 3);
    assert_eq!(UNVIS_SYNBAD, -1);
    assert_eq!(UNVIS_ERROR, -2);
    assert_eq!(UNVIS_END, VIS_END);
}

#[test]
fn re_exports_are_callable() {
    let mut dst = [0u8; 16];
    assert_eq!(strvis(&mut dst, b"a", 0), Some(1));
    assert_eq!(strunvis(&mut dst, b"a", 0), Some(1));
    assert!(vis(&mut dst, b'a', 0, 0).is_some());
    assert!(svis(&mut dst, b'a', 0, 0, b"").is_some());
    assert!(nvis(&mut dst, b'a', 0, 0).is_some());
    assert!(snvis(&mut dst, b'a', 0, 0, b"").is_some());
    assert!(strnvis(&mut dst, b"a", 0).is_some());
    assert!(strsnvis(&mut dst, b"a", 0, b"").is_some());
    assert!(strsvis(&mut dst, b"a", 0, b"").is_some());
    assert!(strvisx(&mut dst, b"a", 1, 0).is_some());
    assert!(strnvisx(&mut dst, b"a", 1, 0).is_some());
    assert!(strsvisx(&mut dst, b"a", 1, 0, b"").is_some());
    assert!(strsnvisx(&mut dst, b"a", 1, 0, b"").is_some());
    assert!(strnunvis(&mut dst, b"a", 0).is_some());
    assert!(strunvisx(&mut dst, b"a", 0).is_some());
    assert!(strnunvisx(&mut dst, b"a", 0).is_some());
}
