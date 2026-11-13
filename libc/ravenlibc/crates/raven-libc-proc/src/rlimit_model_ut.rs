// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the rlimit table.

use super::*;
use crate::consts::RLIMIT_CPU;

#[test]
fn defaults_are_no_file_1024_32768_and_infinity_elsewhere() {
    let table = RlimitTable::new();
    assert_eq!(table.get(RLIMIT_NOFILE), Ok((1024, 32_768)));
    assert_eq!(table.get(RLIMIT_CPU), Ok((RLIM_INFINITY, RLIM_INFINITY)));
}

#[test]
fn resource_sixteen_is_invalid() {
    let table = RlimitTable::new();
    assert_eq!(table.get(16), Err(EINVAL));
}

#[test]
fn set_checks_cur_not_above_max() {
    let mut table = RlimitTable::new();
    assert_eq!(table.set(RLIMIT_CPU, 5, 4, true), Err(EINVAL));
}

#[test]
fn non_root_cannot_raise_the_hard_limit() {
    let mut table = RlimitTable::new();
    assert_eq!(table.set(RLIMIT_NOFILE, 1, 65_536, false), Err(EPERM));
}

#[test]
fn root_can_raise_the_hard_limit() {
    let mut table = RlimitTable::new();
    assert_eq!(table.set(RLIMIT_NOFILE, 1, 65_536, true), Ok(()));
    assert_eq!(table.get(RLIMIT_NOFILE), Ok((1, 65_536)));
}

#[test]
fn lowering_the_hard_limit_is_allowed() {
    let mut table = RlimitTable::new();
    assert_eq!(table.set(RLIMIT_NOFILE, 1, 2_048, false), Ok(()));
    assert_eq!(table.get(RLIMIT_NOFILE), Ok((1, 2_048)));
}

#[test]
fn infinity_round_trips() {
    let mut table = RlimitTable::new();
    assert_eq!(
        table.set(RLIMIT_CPU, RLIM_INFINITY, RLIM_INFINITY, false),
        Ok(())
    );
    assert_eq!(table.get(RLIMIT_CPU), Ok((RLIM_INFINITY, RLIM_INFINITY)));
}

#[test]
fn page_copy_reads_all_sixteen_pairs() {
    let mut page = [0u8; 4096];
    for index in 0..16usize {
        let offset = 200 + index * 16;
        page[offset..offset + 8].copy_from_slice(&(index as u64).to_le_bytes());
        page[offset + 8..offset + 16].copy_from_slice(&((index as u64) + 100).to_le_bytes());
    }
    let table = RlimitTable::get_from_page(&page).expect("page");
    for index in 0..16u32 {
        assert_eq!(
            table.get(index),
            Ok((u64::from(index), u64::from(index) + 100))
        );
    }
}

#[test]
fn page_copy_needs_the_full_page() {
    let page = [0u8; 4096];
    let table = RlimitTable::get_from_page(&page).expect("zero page");
    assert_eq!(table.get(RLIMIT_NOFILE), Ok((0, 0)));
}
