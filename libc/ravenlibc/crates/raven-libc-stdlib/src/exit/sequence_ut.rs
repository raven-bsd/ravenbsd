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

//! The A-stdlib-8 order and the restart/re-entrancy model.

use super::*;
use crate::exit::atexit::{Entry, ExitTable, Kind};

#[derive(Default)]
struct Refs;

impl DsoRefs for Refs {
    fn add_ref(&mut self, _dso: usize) {}
    fn drop_ref(&mut self, _dso: usize) {}
}

fn entry(func: usize) -> Entry {
    Entry {
        func,
        arg: 0,
        dso: 0,
        kind: Kind::Atexit,
    }
}

#[test]
fn thread_list_runs_before_the_table() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    table.register(entry(1)).unwrap();
    table.register(entry(2)).unwrap();
    thread
        .push(
            Entry {
                func: 100,
                arg: 0,
                dso: 5,
                kind: Kind::Cxa,
            },
            &mut refs,
        )
        .unwrap();
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(
        order.iter().map(|entry| entry.func).collect::<Vec<_>>(),
        vec![100, 2, 1]
    );
}

#[test]
fn rtld_first_and_stdio_oldest() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    // The loader registers its finaliser first, so it runs after the user handlers.
    table
        .register(Entry {
            func: 500,
            arg: 0,
            dso: 0,
            kind: Kind::Cxa,
        })
        .unwrap();
    table.register(entry(1)).unwrap();
    table.register(entry(2)).unwrap();
    // The stdio cleanup is registered at the oldest position, so it runs last.
    table
        .register_oldest(Entry {
            func: 900,
            arg: 0,
            dso: 0,
            kind: Kind::Cxa,
        })
        .unwrap();
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(
        order.iter().map(|entry| entry.func).collect::<Vec<_>>(),
        vec![2, 1, 500, 900]
    );
}

#[test]
fn a_handler_registered_during_exit_still_runs() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    table.register(entry(1)).unwrap();
    table.register(entry(2)).unwrap();
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(2));
    table.register(entry(3)).unwrap();
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(
        order.iter().map(|entry| entry.func).collect::<Vec<_>>(),
        vec![3, 1]
    );
}

#[test]
fn reentrant_exit_never_repeats_the_consumed_handler() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    for func in [1usize, 2, 3] {
        table.register(entry(func)).unwrap();
    }
    assert_eq!(table.next_to_run().map(|entry| entry.func), Some(3));
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(
        order.iter().map(|entry| entry.func).collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(table.pending(), 0);
}

#[test]
fn a_thousand_handlers_run_newest_first_once() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    for index in 1..=1000usize {
        table.register(entry(index)).unwrap();
    }
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(order.len(), 1000);
    assert_eq!(order.first().map(|entry| entry.func), Some(1000));
    assert_eq!(order.last().map(|entry| entry.func), Some(1));
    let mut seen: Vec<usize> = order.iter().map(|entry| entry.func).collect();
    seen.sort_unstable();
    assert_eq!(seen, (1..=1000).collect::<Vec<_>>());
}

#[test]
fn empty_exit_produces_nothing() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    assert!(exit_steps(&mut table, &mut thread, &mut refs).is_empty());
}

#[test]
fn thread_list_alone() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    thread
        .push(
            Entry {
                func: 7,
                arg: 0,
                dso: 1,
                kind: Kind::Cxa,
            },
            &mut refs,
        )
        .unwrap();
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(order.len(), 1);
    assert_eq!(order[0].func, 7);
}

#[test]
fn negative_dso_reference_is_preserved() {
    let mut table = ExitTable::new();
    let mut thread = ThreadAtexit::new();
    let mut refs = Refs;
    thread
        .push(
            Entry {
                func: 8,
                arg: 0,
                dso: 0,
                kind: Kind::Atexit,
            },
            &mut refs,
        )
        .unwrap();
    let order = exit_steps(&mut table, &mut thread, &mut refs);
    assert_eq!(order[0].func, 8);
}
