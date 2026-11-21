// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for named address tables.

use super::*;
use crate::address::{AddrSpec, Ipv4Addr};

#[test]
fn a_table_matches_its_entries() {
    let mut table = Table::new("web");
    table.add(AddrSpec::parse("192.0.2.0/24").unwrap()).unwrap();
    table.add(AddrSpec::parse("2001:db8::1").unwrap()).unwrap();
    let tables = [table];
    assert!(tables[0].matches(Ipv4Addr::new(192, 0, 2, 55).into(), &tables));
    assert!(tables[0].matches(
        crate::address::IpAddr::parse("2001:db8::1").unwrap(),
        &tables
    ));
    assert!(!tables[0].matches(Ipv4Addr::new(198, 51, 100, 1).into(), &tables));
}

#[test]
fn an_empty_table_matches_nothing() {
    let table = Table::new("empty");
    assert!(!table.matches(
        Ipv4Addr::new(10, 0, 0, 1).into(),
        core::slice::from_ref(&table)
    ));
}

#[test]
fn nested_tables_are_rejected() {
    let mut table = Table::new("outer");
    assert_eq!(table.add(AddrSpec::Table(0)), Err(TableError::Nested));
}

#[test]
fn a_table_is_bounded() {
    let mut table = Table::new("big");
    let entry = AddrSpec::parse("10.0.0.0/8").unwrap();
    for _ in 0..MAX_TABLE_ENTRIES {
        table.add(entry).unwrap();
    }
    assert_eq!(table.add(entry), Err(TableError::Full));
    assert_eq!(table.entries.len(), MAX_TABLE_ENTRIES);
}

#[test]
fn replace_validates_the_whole_new_list() {
    let mut table = Table::new("t");
    table.add(AddrSpec::parse("10.0.0.1").unwrap()).unwrap();
    assert_eq!(
        table.replace(alloc::vec![AddrSpec::Table(3)]),
        Err(TableError::Nested)
    );
    assert_eq!(table.entries.len(), 1);
    table
        .replace(alloc::vec![
            AddrSpec::parse("192.0.2.1").unwrap(),
            AddrSpec::parse("192.0.2.2").unwrap()
        ])
        .unwrap();
    assert_eq!(table.entries.len(), 2);
}

#[test]
fn the_table_count_is_bounded() {
    let tables: alloc::vec::Vec<Table> = (0..MAX_TABLES)
        .map(|index| Table::new(alloc::format!("t{index}")))
        .collect();
    assert_eq!(
        check_table_capacity(&tables),
        Err(TableError::TooManyTables)
    );
}
