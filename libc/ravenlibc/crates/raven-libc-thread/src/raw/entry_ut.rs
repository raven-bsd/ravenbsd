// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_entry_symbol_exists() {
    assert_ne!(entry_address(), 0);
}

#[test]
fn the_entry_is_inside_the_text_segment() {
    let address = entry_address();
    assert!(address > 0x1000);
    assert!(address < usize::MAX - 0x1000);
}
