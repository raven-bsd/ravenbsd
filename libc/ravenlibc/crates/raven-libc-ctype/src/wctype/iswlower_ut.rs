// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn lower_case_letters_map_up() {
    assert!(iswlower(u32::from(b'a')));
    assert!(!iswlower(u32::from(b'A')));
    assert!(!iswlower(0xaa));
    assert!(!iswlower(u32::MAX));
}
