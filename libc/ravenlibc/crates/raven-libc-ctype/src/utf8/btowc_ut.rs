// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn btowc_narrows_the_int_and_guards_eof() {
    assert_eq!(btowc(MbMode::Utf8, 0x41), 0x41);
    assert_eq!(btowc(MbMode::Utf8, 0x161), 0x61);
    assert_eq!(btowc(MbMode::Utf8, 0xe9), WEOF);
    assert_eq!(btowc(MbMode::Byte, 0xe9), 0xdfe9);
    assert_eq!(btowc(MbMode::Byte, -1), WEOF);
    assert_eq!(btowc(MbMode::Byte, i32::MIN), 0);
    assert_eq!(btowc(MbMode::Byte, i32::MAX), 0xdfff);
}
