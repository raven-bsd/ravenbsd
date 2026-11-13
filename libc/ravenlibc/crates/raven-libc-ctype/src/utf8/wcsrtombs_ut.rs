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
use crate::errors::MbError;

#[test]
fn count_only_leaves_the_position_alone() {
    let src = [0x61u32, 0x20ac, 0];
    let conv = wcsrtombs(None, MbMode::Utf8, &src, None);
    assert_eq!(conv.ret, Ok(4));
    assert_eq!(conv.src, SrcPos::At(0));
}

#[test]
fn never_writes_a_partial_character() {
    let src = [0x20acu32, 0];
    let mut out = [0u8; 2];
    let conv = wcsrtombs(None, MbMode::Utf8, &src, Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(0));
    assert_eq!(conv.src, SrcPos::At(0));
    assert_eq!(out, [0, 0]);
}

#[test]
fn nul_terminates_and_is_not_counted() {
    let src = [0x61u32, 0];
    let mut out = [0u8; 4];
    let conv = wcsrtombs(None, MbMode::Utf8, &src, Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(1));
    assert_eq!(conv.src, SrcPos::Nul);
    assert_eq!(&out[..2], &[0x61, 0]);
}

#[test]
fn unencodable_input_is_ilseq() {
    let src = [0xd800u32, 0];
    let conv = wcsrtombs(None, MbMode::Utf8, &src, None);
    assert_eq!(conv.ret, Err(MbError::Ilseq));
}
