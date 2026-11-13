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

//! The search group surface.

use super::*;

#[test]
fn visit_variants_are_distinct() {
    assert_ne!(Visit::Leaf, Visit::Preorder);
    assert_ne!(Visit::Postorder, Visit::Endorder);
}

#[test]
fn error_variants_are_distinct() {
    assert_ne!(LSearchError::Inval, LSearchError::NoSpace);
    assert_ne!(HSearchError::NoMem, HSearchError::NotFound);
}

#[test]
fn actions_are_comparable() {
    assert_eq!(Action::Find, Action::Find);
    assert_ne!(Action::Find, Action::Enter);
}

#[test]
fn tdelete_result_is_copy() {
    let value = TDelete::Parent(3u8);
    let copied = value;
    assert_eq!(value, copied);
}

#[test]
fn slot_id_is_copy() {
    let slot = SlotId(4);
    assert_eq!(slot, SlotId(4));
}
