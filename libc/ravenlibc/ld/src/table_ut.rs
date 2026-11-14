// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_contract_version_is_one() {
    assert_eq!(RtldTable::VERSION, 1);
}

#[test]
fn the_size_field_matches_the_struct() {
    assert_eq!(RtldTable::SIZE, core::mem::size_of::<RtldTable>() as u32);
}

#[test]
fn the_table_is_repr_c() {
    assert!(core::mem::size_of::<RtldTable>() >= 9 * core::mem::size_of::<usize>());
}
