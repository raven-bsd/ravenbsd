// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn encodes_like_wcrtomb() {
    let mut out = [0u8; 4];
    assert_eq!(c32rtomb(MbMode::Utf8, 0x1f600, &mut out), Ok(4));
    assert_eq!(out, [0xf0, 0x9f, 0x98, 0x80]);
    assert_eq!(
        c32rtomb(MbMode::Utf8, 0x110000, &mut out),
        Err(MbError::Ilseq)
    );
}
