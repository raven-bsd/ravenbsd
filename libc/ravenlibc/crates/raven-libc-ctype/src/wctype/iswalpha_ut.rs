// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn ascii_letters_are_alpha() {
    assert!(iswalpha(u32::from(b'a')));
    assert!(iswalpha(u32::from(b'Z')));
    assert!(!iswalpha(u32::from(b'0')));
    assert!(!iswalpha(0x300));
    assert!(!iswalpha(u32::MAX));
}
