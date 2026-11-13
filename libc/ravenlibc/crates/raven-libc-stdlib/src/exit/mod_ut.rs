// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The exit group surface.

use super::*;

#[test]
fn kinds_and_empty_tables() {
    assert_ne!(Kind::Atexit, Kind::Cxa);
    let mut table = ExitTable::new();
    assert_eq!(table.pending(), 0);
    assert_eq!(table.next_to_run(), None);
    assert_eq!(ExitTable::default(), table);
    let quick = QuickExitTable::new();
    assert!(quick.is_empty());
    let thread = ThreadAtexit::new();
    assert!(thread.is_empty());
}

#[test]
fn entry_is_copy() {
    let entry = Entry {
        func: 1,
        arg: 2,
        dso: 3,
        kind: Kind::Cxa,
    };
    let copied = entry;
    assert_eq!(entry, copied);
}

#[test]
fn quick_default_is_empty() {
    assert_eq!(QuickExitTable::default(), QuickExitTable::new());
}
