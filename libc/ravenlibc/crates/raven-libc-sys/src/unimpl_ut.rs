// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::unimpl`].

#![allow(clippy::indexing_slicing)]

use super::*;

#[test]
fn record_roundtrips_name_and_gap() {
    let record = UnimplRecord::new("getrandom", "G-7", 38);
    assert_eq!(record.magic, MAGIC);
    assert_eq!(record.errno, 38);
    assert_eq!(record.name_str(), "getrandom");
    assert_eq!(record.gap_str(), "G-7");
}

#[test]
fn long_name_is_truncated_and_terminated() {
    let long = "x".repeat(200);
    let leaked: &'static str = host::boxed::Box::leak(long.into_boxed_str());
    let record = UnimplRecord::new(leaked, "G-123456789012345", 1);
    assert_eq!(record.name_str().len(), NAME_LEN - 1);
    assert_eq!(record.gap_str().len(), GAP_LEN - 1);
    assert_eq!(record.name[NAME_LEN - 1], 0);
    assert_eq!(record.gap[GAP_LEN - 1], 0);
}

#[test]
fn fail_returns_minus_one() {
    assert_eq!(fail(38), -1);
}

#[test]
fn empty_strings_are_valid() {
    let record = UnimplRecord::new("", "", 0);
    assert_eq!(record.name_str(), "");
    assert_eq!(record.gap_str(), "");
}

#[test]
fn record_is_repr_c_and_fixed_size() {
    assert_eq!(
        core::mem::size_of::<UnimplRecord>(),
        4 + 4 + NAME_LEN + GAP_LEN
    );
}
