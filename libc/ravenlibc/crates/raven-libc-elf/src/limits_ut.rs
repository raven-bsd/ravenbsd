// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(clippy::assertions_on_constants)]
// The caps are constants; the assertions document and re-state them.

//! Cap sanity.

use super::*;

#[test]
fn the_caps_are_ordered_and_positive() {
    assert!(MAX_PHDRS > 0);
    assert!(MAX_LOAD_SEGMENTS <= MAX_PHDRS);
    assert!(MAX_NEEDED > 0);
    assert!(MAX_DYN_ENTRIES > MAX_NEEDED);
    assert!(MAX_SYMBOLS >= MAX_RELOCS_PER_OBJECT / 4);
    assert!(MAX_VERSIONS < MAX_SYMBOLS);
    assert!(MAX_SEARCH_PATH_LEN > MAX_PATH_LEN);
    assert!(MAX_SEARCH_CANDIDATES > 0);
    assert!(MAX_INIT_ARRAY > 0);
    assert!(MAX_SCOPE_OBJECTS > 0);
}

#[test]
fn the_documented_values_are_stable() {
    assert_eq!(MAX_PHDRS, 64);
    assert_eq!(MAX_LOAD_SEGMENTS, 16);
    assert_eq!(MAX_NEEDED, 256);
    assert_eq!(MAX_DYN_ENTRIES, 512);
    assert_eq!(MAX_RELOCS_PER_OBJECT, 4_000_000);
    assert_eq!(MAX_SYMBOLS, 1_000_000);
    assert_eq!(MAX_VERSIONS, 4096);
    assert_eq!(MAX_SEARCH_PATH_LEN, 65_536);
    assert_eq!(MAX_SEARCH_CANDIDATES, 1024);
    assert_eq!(MAX_PATH_LEN, 4096);
}
