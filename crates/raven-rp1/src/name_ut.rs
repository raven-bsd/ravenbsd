// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the inline name type.

use super::*;

#[test]
fn a_short_name_round_trips() {
    let name = Name::new("uart0").expect("name");
    assert_eq!(name.as_str(), "uart0");
    assert_eq!(name.len(), 5);
    assert!(!name.is_empty());
}

#[test]
fn the_longest_name_fits() {
    let text = "0123456789abcdef";
    assert_eq!(Name::new(text).expect("name").as_str(), text);
}

#[test]
fn an_empty_name_is_rejected() {
    assert_eq!(Name::new(""), Err(NameError::Empty));
}

#[test]
fn an_oversized_name_is_rejected_not_truncated() {
    assert_eq!(Name::new("0123456789abcdefg"), Err(NameError::TooLong(17)));
}

#[test]
fn names_compare_by_content() {
    assert_eq!(
        Name::new("i2c0").expect("name"),
        Name::new("i2c0").expect("name")
    );
    assert_ne!(
        Name::new("i2c0").expect("name"),
        Name::new("i2c1").expect("name")
    );
}
