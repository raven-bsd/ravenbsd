// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn mblen_returns_the_consumed_length() {
    assert_eq!(mblen(MbMode::Utf8, b"A"), Ok(1));
    assert_eq!(mblen(MbMode::Utf8, &[0xc3, 0xa9]), Ok(2));
    assert_eq!(mblen(MbMode::Utf8, &[0xe2, 0x82, 0xac]), Ok(3));
    assert_eq!(mblen(MbMode::Utf8, &[0xf0, 0x9f, 0x92, 0xa9]), Ok(4));
    assert_eq!(mblen(MbMode::Utf8, &[]), Err(MbError::Ilseq));
    assert_eq!(mblen(MbMode::Utf8, &[0xc3]), Err(MbError::Ilseq));
}
