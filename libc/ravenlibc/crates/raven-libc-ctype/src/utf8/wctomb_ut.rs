// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn wctomb_delegates_to_wcrtomb() {
    let mut out = [0u8; 4];
    assert_eq!(wctomb(MbMode::Utf8, 0x41, &mut out), Ok(1));
    assert_eq!(out.first(), Some(&0x41));
    assert_eq!(wctomb(MbMode::Utf8, 0xd800, &mut out), Err(MbError::Ilseq));
}
