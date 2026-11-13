// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn list_has_21_entries_and_excludes_nbsp() {
    assert_eq!(SPACES.len(), 21);
    assert!(iswspace(0x20));
    assert!(iswspace(0x85));
    assert!(iswspace(0x2000));
    assert!(iswspace(0x3000));
    assert!(!iswspace(0));
    assert!(!iswspace(0xa0));
    assert!(!iswspace(0x2007));
    assert!(!iswspace(0x202f));
    assert!(!iswspace(0x1680));
    assert!(!iswspace(0x180e));
}
