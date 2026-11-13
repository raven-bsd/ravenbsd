// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::ofl`]: list insertion/removal, the lock word, and lock order.
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

#[test]
fn add_and_remove_reverse_order() {
    let state = leaked_state();
    let a = leaked_file(state);
    let b = leaked_file(state);
    let c = leaked_file(state);
    add(state, a);
    add(state, b);
    add(state, c);
    let guard = OflGuard::acquire(state);
    let head = guard.head().expect("head");
    assert!(core::ptr::eq(head, c));
    let second = head.st().next.expect("second");
    assert!(core::ptr::eq(second, b));
    let third = second.st().next.expect("third");
    assert!(core::ptr::eq(third, a));
    assert!(third.st().next.is_none());
    drop(guard);
    remove(state, b);
    let guard = OflGuard::acquire(state);
    assert!(core::ptr::eq(guard.head().expect("head"), c));
    assert!(c.st().next.is_some_and(|next| core::ptr::eq(next, a)));
    drop(guard);
    remove(state, c);
    remove(state, a);
    assert!(state.ofl_head().is_none());
}

#[test]
fn lock_word_returns_to_zero() {
    let state = leaked_state();
    {
        let _guard = OflGuard::acquire(state);
        assert_eq!(
            state
                .ofl_lock_word()
                .load(core::sync::atomic::Ordering::Relaxed),
            1
        );
    }
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn list_lock_before_file_lock_is_not_an_inversion() {
    let state = leaked_state();
    let file = leaked_file(state);
    crate::fake_ut::order::reset();
    {
        let _list = OflGuard::acquire(state);
        let _file = crate::lock::FileGuard::new(file);
    }
    assert_eq!(crate::fake_ut::order::inversions(), 0);
}

#[test]
fn file_then_list_is_an_inversion() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    crate::fake_ut::order::reset();
    {
        let _file = crate::lock::FileGuard::new(file);
        let _list = OflGuard::acquire(state);
    }
    assert_eq!(crate::fake_ut::order::inversions(), 1);
    crate::fake_ut::order::reset();
}

#[test]
fn fork_helpers_leave_the_word_unlocked_in_the_child() {
    let state = leaked_state();
    crate::exit::fork_prepare(state);
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        1
    );
    crate::exit::fork_child(state);
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        0
    );
    crate::exit::fork_prepare(state);
    crate::exit::fork_parent(state);
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}
