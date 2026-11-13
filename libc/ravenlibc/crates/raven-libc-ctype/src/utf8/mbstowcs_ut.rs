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
    let conv = mbstowcs(MbMode::Utf8, b"ab\0", None);
    assert_eq!(conv.ret, Ok(2));
    let mut out = [0u32; 4];
    let conv = mbstowcs(MbMode::Utf8, b"ab\0", Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(2));
    assert_eq!(out[0], 0x61);
    assert_eq!(conv.src, SrcPos::Nul);
}
