// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the monotonic pledge reduction.

use super::*;
use crate::errors::EINVAL;

fn set(text: &[u8]) -> PromiseSet {
    parse_promises(text).expect("promises")
}

#[test]
fn a_new_state_is_unpledged() {
    let state = PledgeState::new();
    assert_eq!(state.promises, None);
    assert_eq!(state.exec_promises, None);
}

#[test]
fn the_first_call_sets_the_promises() {
    let mut state = PledgeState::new();
    assert_eq!(
        state.apply(Some(b"stdio rpath"), None),
        Ok(PledgeOutcome::default())
    );
    assert_eq!(state.promises, Some(set(b"stdio rpath")));
}

#[test]
fn the_first_call_can_set_exec_promises() {
    let mut state = PledgeState::new();
    state.apply(None, Some(b"stdio")).expect("apply");
    assert_eq!(state.promises, None);
    assert_eq!(state.exec_promises, Some(set(b"stdio")));
}

#[test]
fn a_superset_is_eperm() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio"), None).expect("first");
    assert_eq!(state.apply(Some(b"stdio rpath"), None), Err(EPERM));
    assert_eq!(state.promises, Some(set(b"stdio")));
}

#[test]
fn an_equal_set_is_allowed() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio rpath"), None).expect("first");
    assert_eq!(
        state.apply(Some(b"rpath stdio"), None),
        Ok(PledgeOutcome::default())
    );
}

#[test]
fn a_strict_subset_is_allowed() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio rpath inet"), None).expect("first");
    state.apply(Some(b"rpath"), None).expect("second");
    assert_eq!(state.promises, Some(set(b"rpath")));
}

#[test]
fn a_none_argument_leaves_that_set_unchanged() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio"), Some(b"inet")).expect("first");
    state.apply(None, None).expect("second");
    assert_eq!(state.promises, Some(set(b"stdio")));
    assert_eq!(state.exec_promises, Some(set(b"inet")));
}

#[test]
fn parse_errors_are_atomic_for_both_sets() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio"), Some(b"inet")).expect("first");
    assert_eq!(
        state.apply(Some(b"stdio unknown"), Some(b"wpath")),
        Err(EINVAL)
    );
    assert_eq!(state.promises, Some(set(b"stdio")));
    assert_eq!(state.exec_promises, Some(set(b"inet")));
}

#[test]
fn an_invalid_exec_string_does_not_change_promises() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio"), None).expect("first");
    assert_eq!(state.apply(Some(b"rpath"), Some(b"unknown")), Err(EINVAL));
    assert_eq!(state.promises, Some(set(b"stdio")));
}

#[test]
fn error_mode_masks_an_increase() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio error"), None).expect("first");
    state.apply(Some(b"stdio error rpath"), None).expect("mask");
    assert_eq!(state.promises, Some(set(b"stdio error")));
}

#[test]
fn error_mode_does_not_apply_to_exec_promises() {
    let mut state = PledgeState::new();
    state
        .apply(Some(b"stdio error"), Some(b"stdio"))
        .expect("first");
    assert_eq!(state.apply(None, Some(b"stdio rpath")), Err(EPERM));
}

#[test]
fn exec_reduction_is_separate() {
    let mut state = PledgeState::new();
    state.apply(Some(b"stdio"), Some(b"rpath")).expect("first");
    assert_eq!(
        state.apply(Some(b"stdio"), Some(b"rpath wpath")),
        Err(EPERM)
    );
    assert_eq!(state.exec_promises, Some(set(b"rpath")));
    state.apply(None, Some(b"rpath")).expect("reduce");
    assert_eq!(state.exec_promises, Some(set(b"rpath")));
}

#[test]
fn a_stdio_only_set_asks_for_unveil_cleanup() {
    let mut state = PledgeState::new();
    let outcome = state.apply(Some(b"stdio"), None).expect("apply");
    assert!(outcome.unveil_cleanup);
}

#[test]
fn path_promises_keep_unveil() {
    for text in [
        b"rpath".as_slice(),
        b"wpath".as_slice(),
        b"cpath".as_slice(),
        b"dpath".as_slice(),
        b"exec".as_slice(),
        b"unix".as_slice(),
        b"unveil".as_slice(),
    ] {
        let mut state = PledgeState::new();
        let outcome = state.apply(Some(text), None).expect("apply");
        assert!(!outcome.unveil_cleanup, "{text:?}");
    }
}

#[test]
fn removing_every_keep_promise_asks_for_cleanup() {
    let mut state = PledgeState::new();
    state.apply(Some(b"rpath inet"), None).expect("first");
    let outcome = state.apply(Some(b"inet"), None).expect("second");
    assert!(outcome.unveil_cleanup);
}

#[test]
fn an_unpledged_state_does_not_ask_for_cleanup() {
    let mut state = PledgeState::new();
    let outcome = state.apply(None, Some(b"stdio")).expect("apply");
    assert!(!outcome.unveil_cleanup);
}
