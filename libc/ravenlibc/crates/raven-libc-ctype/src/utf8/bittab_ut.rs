// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;
use crate::utf8::internal::oob;

#[test]
fn table_has_51_entries_and_decodes_every_lead() {
    assert_eq!(BITTAB.len(), 51);
    for (index, value) in BITTAB.iter().enumerate() {
        assert_eq!(entry((SA as usize + index) as u8), Some(*value));
    }
    assert_eq!(entry(0xc1), None);
    assert_eq!(entry(0xf5), None);
}

#[test]
fn first_two_byte_entry_accepts_only_continuations() {
    let state = BITTAB[0];
    assert_eq!(state, 0xc000_0002);
    assert!(!oob(state, 0x80));
    assert!(!oob(state, 0xbf));
    assert!(oob(state, 0x7f));
    assert!(oob(state, 0xc0));
}

#[test]
fn code_unit_helper_matches_internal_h() {
    assert!(code_unit(0xdf80));
    assert!(code_unit(0xdfff));
    assert!(!code_unit(0xe000));
    assert!(is_continuation(0x80));
    assert!(is_continuation(0xbf));
    assert!(!is_continuation(0x7f));
    assert!(!is_continuation(0xc0));
}
