// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn digits_and_letters_are_alnum() {
    assert!(iswalnum(u32::from(b'a')));
    assert!(iswalnum(u32::from(b'7')));
    assert!(!iswalnum(u32::from(b'_')));
    assert!(!iswalnum(u32::MAX));
}
