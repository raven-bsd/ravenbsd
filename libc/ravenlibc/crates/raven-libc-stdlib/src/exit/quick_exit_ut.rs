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

//! `quick_exit`: its own LIFO list and no `atexit` handlers.

use super::*;
use crate::exit::{Entry, ExitTable, Kind};

#[test]
fn lifo_order() {
    let mut table = QuickExitTable::new();
    for func in [1usize, 2, 3] {
        table.register(func).unwrap();
    }
    assert_eq!(table.len(), 3);
    let steps = quick_exit_steps(&mut table);
    assert_eq!(
        steps.iter().map(|entry| entry.func).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert!(table.is_empty());
    assert!(quick_exit_steps(&mut table).is_empty());
}

#[test]
fn separate_from_the_atexit_table() {
    let mut quick = QuickExitTable::new();
    let mut regular = ExitTable::new();
    quick.register(7).unwrap();
    regular
        .register(Entry {
            func: 8,
            arg: 0,
            dso: 0,
            kind: Kind::Atexit,
        })
        .unwrap();
    assert_eq!(quick.len(), 1);
    assert_eq!(regular.pending(), 1);
    quick_exit_steps(&mut quick);
    assert_eq!(regular.pending(), 1);
}

#[test]
fn empty_quick_exit() {
    let mut table = QuickExitTable::new();
    assert!(quick_exit_steps(&mut table).is_empty());
    assert_eq!(QuickExitTable::default(), table);
}

#[test]
fn one_handler() {
    let mut table = QuickExitTable::new();
    table.register(5).unwrap();
    let steps = quick_exit_steps(&mut table);
    assert_eq!(steps[0].func, 5);
    assert_eq!(steps[0].kind, Kind::Atexit);
}
