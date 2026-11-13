// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn mb_cur_max_is_mode_dependent() {
    assert_eq!(MbMode::Byte.mb_cur_max(), 1);
    assert_eq!(MbMode::Utf8.mb_cur_max(), 4);
    assert_eq!(MB_CUR_MAX_BYTE, 1);
    assert_eq!(MB_CUR_MAX_UTF8, 4);
}

#[test]
fn eof_and_weof_values() {
    assert_eq!(EOF, -1);
    assert_eq!(WEOF, u32::MAX);
}
