// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic
)]

use super::*;
use crate::utf8::SrcPos;

#[test]
fn count_and_convert() {
    let src = [0x61u32, 0x20ac, 0];
    let conv = wcstombs(MbMode::Utf8, &src, None);
    assert_eq!(conv.ret, Ok(4));
    let mut out = [0u8; 8];
    let conv = wcstombs(MbMode::Utf8, &src, Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(4));
    assert_eq!(&out[..4], &[0x61, 0xe2, 0x82, 0xac]);
    assert_eq!(conv.src, SrcPos::Nul);
}
