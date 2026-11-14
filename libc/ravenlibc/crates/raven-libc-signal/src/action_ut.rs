// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::dispositions::{SA_ONSTACK, SA_RESETHAND, SA_SIGINFO};

#[test]
fn kill_and_stop_refuse_a_new_action() {
    let table = DispTable::new();
    let action = Disposition::IGNORE;
    assert_eq!(sigaction(&table, 9, Some(&action), None), Err(EINVAL_ERR));
    assert_eq!(sigaction(&table, 19, Some(&action), None), Err(EINVAL_ERR));
    assert_eq!(table.read(9), Disposition::DEFAULT);
}

#[test]
fn internal_signals_are_refused() {
    let table = DispTable::new();
    let action = Disposition::IGNORE;
    for sig in [0, -1, 32, 33, 34, 65, 1000] {
        assert_eq!(sigaction(&table, sig, Some(&action), None), Err(EINVAL_ERR));
    }
}

#[test]
fn old_reports_exactly_what_was_stored() {
    let table = DispTable::new();
    let first = Disposition {
        handler: 0x1234,
        mask: APP_MASK & 0x0000_0000_0000_00ff,
        flags: SA_SIGINFO | SA_ONSTACK,
    };
    sigaction(&table, 11, Some(&first), None).unwrap();
    let second = Disposition {
        handler: 0x5678,
        mask: 1 << 4,
        flags: SA_RESETHAND,
    };
    let mut old = Disposition::DEFAULT;
    sigaction(&table, 11, Some(&second), Some(&mut old)).unwrap();
    assert_eq!(old, first);
    assert_eq!(table.read(11), second);
}

#[test]
fn sa_restorer_is_dropped_and_never_stored() {
    let table = DispTable::new();
    let action = Disposition {
        handler: 0x9999,
        mask: 0,
        flags: SA_RESTART | SA_RESTORER,
    };
    sigaction(&table, 15, Some(&action), None).unwrap();
    assert_eq!(table.read(15).flags, SA_RESTART);
}

#[test]
fn disabled_bits_are_cleared_in_the_stored_mask() {
    let table = DispTable::new();
    let action = Disposition {
        handler: 0x1000,
        mask: u64::MAX,
        flags: 0,
    };
    sigaction(&table, 13, Some(&action), None).unwrap();
    assert_eq!(table.read(13).mask, APP_MASK);
}

#[test]
fn handler_set_tracks_function_handlers_only() {
    let table = DispTable::new();
    sigaction(&table, 1, Some(&Disposition::IGNORE), None).unwrap();
    assert!(!table.handler_set().is_member(1).unwrap());
    sigaction(&table, 2, Some(&Disposition::handler(0x2000)), None).unwrap();
    assert!(table.handler_set().is_member(2).unwrap());
}

#[test]
fn install_without_restart_sets_eintr_valid() {
    let table = DispTable::new();
    clear_eintr_valid();
    sigaction(&table, 14, Some(&Disposition::handler(0x3000)), None).unwrap();
    assert!(eintr_valid());
}

#[test]
fn query_without_new_is_a_read() {
    let table = DispTable::new();
    let mut old = Disposition::IGNORE;
    sigaction(&table, 7, None, Some(&mut old)).unwrap();
    assert_eq!(old, Disposition::DEFAULT);
}
