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

//! The exit table: LIFO, consume-before-return, the inline 32 and DSO/range selection.

use super::*;

fn entry(func: usize) -> Entry {
    Entry {
        func,
        arg: 0,
        dso: 0,
        kind: Kind::Atexit,
    }
}

#[test]
fn newest_first_exactly_once() {
    let mut table = ExitTable::new();
    for value in 1..=1000usize {
        table.register(entry(value)).unwrap();
    }
    assert_eq!(table.pending(), 1000);
    let mut seen = Vec::new();
    while let Some(next) = table.next_to_run() {
        seen.push(next.func);
    }
    assert_eq!(seen, (1..=1000).rev().collect::<Vec<_>>());
    assert_eq!(table.pending(), 0);
    assert_eq!(table.next_to_run(), None);
}

#[test]
fn the_first_32_are_inline() {
    let mut table = ExitTable::new();
    assert_eq!(table.inline_capacity(), 32);
    for value in 1..=32usize {
        table.register(entry(value)).unwrap();
    }
    assert!(table.spill.is_empty());
    table.register(entry(33)).unwrap();
    assert_eq!(table.spill.len(), 1);
    assert_eq!(table.pending(), 33);
}

#[test]
fn newest_after_a_spill_is_still_newest() {
    let mut table = ExitTable::new();
    for value in 1..=40usize {
        table.register(entry(value)).unwrap();
    }
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(40));
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(39));
}

#[test]
fn oldest_registration_runs_last() {
    let mut table = ExitTable::new();
    let stdio = Entry {
        func: 999,
        arg: 0,
        dso: 0,
        kind: Kind::Cxa,
    };
    table.register_oldest(stdio).unwrap();
    table.register(entry(1)).unwrap();
    table.register(entry(2)).unwrap();
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(2));
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(1));
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(999));
}

#[test]
fn oldest_with_a_full_inline_table_displaces_the_youngest_inline() {
    let mut table = ExitTable::new();
    for value in 1..=40usize {
        table.register(entry(value)).unwrap();
    }
    table.register_oldest(entry(999)).unwrap();
    let mut order = Vec::new();
    while let Some(entry) = table.next_to_run() {
        order.push(entry.func);
    }
    assert_eq!(order.first(), Some(&40));
    assert_eq!(order.last(), Some(&999));
    assert!(order.contains(&32));
}

#[test]
fn finalize_dso_selects_only_that_handle() {
    let mut table = ExitTable::new();
    for (func, dso) in [(1usize, 7usize), (2, 8), (3, 7), (4, 0)] {
        table
            .register(Entry {
                func,
                arg: 0,
                dso,
                kind: Kind::Cxa,
            })
            .unwrap();
    }
    assert_eq!(table.finalize_dso(7).map(|entry| entry.func), Some(3));
    assert_eq!(table.finalize_dso(7).map(|entry| entry.func), Some(1));
    assert_eq!(table.finalize_dso(7), None);
    assert_eq!(table.finalize_dso(0), None);
    assert_eq!(table.pending(), 2);
}

#[test]
fn finalize_range_selects_by_address() {
    let mut table = ExitTable::new();
    for func in [0x1000usize, 0x2000, 0x3000] {
        table.register(entry(func)).unwrap();
    }
    assert_eq!(
        table
            .finalize_range(&[(0x2000, 0x1000)])
            .map(|entry| entry.func),
        Some(0x2000)
    );
    assert_eq!(
        table
            .finalize_range(&[(0x0fff, 2), (0x3000, 1)])
            .map(|entry| entry.func),
        Some(0x3000)
    );
    assert_eq!(table.pending(), 1);
    assert_eq!(table.finalize_range(&[(0x2000, 0x1000)]), None);
}

#[test]
fn pending_is_zero_after_drain() {
    let mut table = ExitTable::new();
    table.register(entry(1)).unwrap();
    table.next_to_run();
    assert_eq!(table.pending(), 0);
}

#[test]
fn register_oldest_on_empty() {
    let mut table = ExitTable::new();
    table.register_oldest(entry(9)).unwrap();
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(9));
}

#[test]
fn dso_zero_never_matches() {
    let mut table = ExitTable::new();
    table.register(entry(1)).unwrap();
    assert_eq!(table.finalize_dso(0), None);
    assert_eq!(table.pending(), 1);
}

#[test]
fn range_with_zero_length_never_matches() {
    let mut table = ExitTable::new();
    table.register(entry(0x1000)).unwrap();
    assert_eq!(table.finalize_range(&[(0x1000, 0)]), None);
}

#[test]
fn consumed_entry_skipped_by_dso_and_range() {
    let mut table = ExitTable::new();
    table
        .register(Entry {
            func: 5,
            arg: 0,
            dso: 3,
            kind: Kind::Cxa,
        })
        .unwrap();
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(5));
    assert_eq!(table.finalize_dso(3), None);
    assert_eq!(table.finalize_range(&[(5, 1)]), None);
}

#[test]
fn default_table_is_empty() {
    assert_eq!(ExitTable::default(), ExitTable::new());
}
