// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::state`].
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::bool_assert_comparison,
    clippy::undocumented_unsafe_blocks,
    clippy::uninlined_format_args
)]

use super::*;
use crate::fake_ut::{FakeEnv, leaked_file, leaked_state};
use crate::traits::SysFile;

#[test]
fn mark_used_is_true_exactly_once() {
    let state = leaked_state();
    assert!(!state.is_used());
    assert!(state.mark_used());
    assert!(!state.mark_used());
    assert!(state.is_used());
}

#[test]
fn env_is_the_constructor_argument() {
    let env = FakeEnv::new();
    let state: &'static State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(State::new(env)));
    state.env().add_file(b"/x", b"y", 512);
    assert_eq!(state.env().open(b"/x", 0, 0), Ok(3));
}

#[test]
fn open_file_list_links_and_head() {
    let state = leaked_state();
    assert!(state.ofl_head().is_none());
    let a = leaked_file(state);
    let b = leaked_file(state);
    crate::ofl::add(state, a);
    assert!(state.ofl_head().is_some_and(|head| core::ptr::eq(head, a)));
    crate::ofl::add(state, b);
    assert!(state.ofl_head().is_some_and(|head| core::ptr::eq(head, b)));
    assert!(a.st().prev.is_some_and(|prev| core::ptr::eq(prev, b)));
    assert!(b.st().next.is_some_and(|next| core::ptr::eq(next, a)));
    crate::ofl::remove(state, b);
    assert!(state.ofl_head().is_some_and(|head| core::ptr::eq(head, a)));
    assert!(a.st().prev.is_none());
    crate::ofl::remove(state, a);
    assert!(state.ofl_head().is_none());
}

#[test]
fn standard_slots_are_idempotent() {
    let state = leaked_state();
    let file = leaked_file(state);
    assert!(state.std_file(StdKind::In).is_none());
    state.bind_std(StdKind::In, file);
    assert!(
        state
            .std_file(StdKind::In)
            .is_some_and(|slot| core::ptr::eq(slot, file))
    );
    state.bind_std(StdKind::Out, file);
    assert!(state.std_file(StdKind::Out).is_some());
    assert!(state.std_file(StdKind::Err).is_none());
}
