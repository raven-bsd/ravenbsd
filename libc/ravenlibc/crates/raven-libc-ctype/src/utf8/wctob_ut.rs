// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn wctob_round_trips_code_units_only() {
    assert_eq!(wctob(MbMode::Utf8, 0x41), 0x41);
    assert_eq!(wctob(MbMode::Utf8, 0xdfe9), EOF);
    assert_eq!(wctob(MbMode::Byte, 0xdfe9), 0xe9);
    assert_eq!(wctob(MbMode::Byte, 0xe9), EOF);
    assert_eq!(wctob(MbMode::Byte, u32::MAX), EOF);
}
