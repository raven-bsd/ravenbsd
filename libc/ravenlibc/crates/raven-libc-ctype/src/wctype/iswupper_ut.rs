// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn upper_case_letters_map_down() {
    assert!(iswupper(u32::from(b'A')));
    assert!(!iswupper(u32::from(b'a')));
    assert!(!iswupper(u32::MAX));
}
