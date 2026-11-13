// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stringlist` tests: add/find/items, the C growth boundary at 20 entries, case folding.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

fn list(items: &[&[u8]]) -> StringList {
    let mut sl = StringList::new();
    for item in items {
        sl.add(item.to_vec()).unwrap();
    }
    sl
}

#[test]
fn new_is_empty() {
    let sl = StringList::new();
    assert!(sl.is_empty());
    assert_eq!(sl.len(), 0);
    assert!(sl.items().is_empty());
}

#[test]
fn add_appends_in_order() {
    let sl = list(&[b"a", b"bc", b""]);
    assert_eq!(sl.len(), 3);
    assert_eq!(sl.items()[0], b"a");
    assert_eq!(sl.items()[2], b"");
}

#[test]
fn find_linear_exact() {
    let sl = list(&[b"/bin/sh", b"/bin/csh"]);
    assert_eq!(sl.find(b"/bin/sh", false), Some(0));
    assert_eq!(sl.find(b"/bin/csh", false), Some(1));
    assert_eq!(sl.find(b"/bin/zsh", false), None);
    assert_eq!(sl.find(b"/BIN/SH", false), None);
}

#[test]
fn find_case_insensitive() {
    let sl = list(&[b"Alpha", b"beta"]);
    assert_eq!(sl.find(b"alpha", true), Some(0));
    assert_eq!(sl.find(b"BETA", true), Some(1));
    assert_eq!(sl.find(b"gamma", true), None);
}

#[test]
fn duplicate_names_return_the_first() {
    let sl = list(&[b"x", b"x"]);
    assert_eq!(sl.find(b"x", false), Some(0));
}

#[test]
fn twenty_first_entry_like_the_c_growth_chunk() {
    let mut sl = StringList::new();
    for index in 0..40u32 {
        sl.add(alloc::format!("s{index}").into_bytes()).unwrap();
    }
    assert_eq!(sl.len(), 40);
    assert_eq!(sl.find(b"s20", false), Some(20));
    assert_eq!(sl.find(b"s39", false), Some(39));
}

#[test]
fn clear_drops_everything() {
    let mut sl = list(&[b"a"]);
    sl.clear();
    assert!(sl.is_empty());
    assert!(sl.items().is_empty());
}

#[test]
fn non_utf8_bytes_are_stored_verbatim() {
    let sl = list(&[&[0xff, 0x00, 0x41]]);
    assert_eq!(sl.items()[0], &[0xff, 0x00, 0x41]);
    assert_eq!(sl.find(&[0xff, 0x00, 0x41], false), Some(0));
}
