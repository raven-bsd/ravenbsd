// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn punctuation_and_symbols() {
    assert!(iswpunct(u32::from(b'!')));
    assert!(!iswpunct(u32::from(b'a')));
    assert!(!iswpunct(0x3000));
    assert!(iswpunct(0x20ac));
}
