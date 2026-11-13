// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn stateless_decode() {
    assert_eq!(mbtowc(MbMode::Utf8, b"A"), Ok((0x41, 1)));
    assert_eq!(mbtowc(MbMode::Utf8, &[0xc3, 0xa9]), Ok((0xe9, 2)));
    assert_eq!(mbtowc(MbMode::Utf8, &[0xe2, 0x82, 0xac]), Ok((0x20ac, 3)));
    assert_eq!(
        mbtowc(MbMode::Utf8, &[0xf0, 0x9f, 0x98, 0x80]),
        Ok((0x1f600, 4))
    );
}

#[test]
fn rejects_empty_truncated_and_invalid() {
    assert_eq!(mbtowc(MbMode::Utf8, &[]), Err(MbError::Ilseq));
    assert_eq!(mbtowc(MbMode::Utf8, &[0xc3]), Err(MbError::Ilseq));
    assert_eq!(
        mbtowc(MbMode::Utf8, &[0xe0, 0x80, 0x80]),
        Err(MbError::Ilseq)
    );
    assert_eq!(
        mbtowc(MbMode::Utf8, &[0xf4, 0x90, 0x80, 0x80]),
        Err(MbError::Ilseq)
    );
}

#[test]
fn byte_mode_is_transparent() {
    assert_eq!(mbtowc(MbMode::Byte, &[0xe9]), Ok((0xdfe9, 1)));
}
