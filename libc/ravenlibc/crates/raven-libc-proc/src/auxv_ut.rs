// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the auxiliary-vector table.

use super::*;

#[test]
fn an_empty_table_has_no_entries() {
    let table = AuxTable::new();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
    assert_eq!(table.get(6), None);
}

#[test]
fn a_key_returns_its_value() {
    let mut table = AuxTable::new();
    table.insert(6, 4096);
    assert_eq!(table.get(6), Some(4096));
    assert_eq!(table.len(), 1);
}

#[test]
fn the_first_occurrence_wins() {
    let mut table = AuxTable::new();
    table.insert(6, 4096);
    table.insert(6, 8192);
    assert_eq!(table.get(6), Some(4096));
    assert_eq!(table.len(), 1);
}

#[test]
fn iteration_keeps_insertion_order() {
    let mut table = AuxTable::new();
    table.insert(3, 1);
    table.insert(6, 2);
    let pairs: alloc::vec::Vec<_> = table.iter().collect();
    assert_eq!(pairs, alloc::vec![(3, 1), (6, 2)]);
}

#[test]
fn unknown_keys_are_stored() {
    let mut table = AuxTable::new();
    table.insert(0x1234, 0x5678);
    assert_eq!(table.get(0x1234), Some(0x5678));
}

#[test]
fn a_zero_value_is_distinct_from_absence() {
    let mut table = AuxTable::new();
    table.insert(3, 0);
    assert_eq!(table.get(3), Some(0));
    assert_eq!(table.get(4), None);
}
