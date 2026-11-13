// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_new_state_is_single_threaded() {
    let state = LibcThreads::new();
    assert_eq!(state.threads(), 1);
    assert_eq!(state.need_locks(), 0);
}

#[test]
fn adding_the_second_thread_enables_locks() {
    let state = LibcThreads::new();
    assert_eq!(state.add_thread(), 2);
    assert_eq!(state.need_locks(), 1);
}

#[test]
fn removing_the_last_thread_marks_the_lock_skip() {
    let state = LibcThreads::new();
    state.add_thread();
    assert_eq!(state.remove_thread(), 1);
    assert_eq!(state.need_locks(), 1);
    assert_eq!(state.remove_thread(), 0);
    assert_eq!(state.need_locks(), -1);
}

#[test]
fn reset_restores_one_thread_and_clears_locks() {
    let state = LibcThreads::new();
    state.add_thread();
    state.set_need_locks(-1);
    state.reset(0x1234);
    assert_eq!(state.threads(), 1);
    assert_eq!(state.need_locks(), 0);
    assert_eq!(state.main_tcb(), 0x1234);
}

#[test]
fn tids_are_nonzero_and_below_thirty_bits() {
    let state = LibcThreads::new();
    for _ in 0..1000 {
        let tid = state.next_tid();
        assert!(tid > 0);
        assert_eq!(tid & !0x3fff_ffff, 0);
        assert_ne!(tid, 0x3fff_ffff);
    }
}

#[test]
fn the_defaults_start_at_the_open_item_values() {
    let state = LibcThreads::new();
    assert_eq!(state.default_stack(), DEFAULT_STACK_SIZE);
    assert_eq!(state.default_guard(), DEFAULT_GUARD_SIZE);
}

#[test]
fn defaults_only_grow() {
    let state = LibcThreads::new();
    state.set_defaults(1024, 8);
    assert_eq!(state.default_stack(), DEFAULT_STACK_SIZE);
    assert_eq!(state.default_guard(), DEFAULT_GUARD_SIZE);
    state.set_defaults(DEFAULT_STACK_SIZE * 2, DEFAULT_GUARD_SIZE * 2);
    assert_eq!(state.default_stack(), DEFAULT_STACK_SIZE * 2);
    assert_eq!(state.default_guard(), DEFAULT_GUARD_SIZE * 2);
}

#[test]
fn exit_hook_is_installable() {
    let state = LibcThreads::new();
    assert_eq!(state.exit_hook(), 0);
    state.set_exit_hook(0x1234);
    assert_eq!(state.exit_hook(), 0x1234);
    state.set_exit_hook(0);
}

#[test]
fn set_threads_is_the_fork_child_path() {
    let state = LibcThreads::new();
    state.add_thread();
    state.add_thread();
    state.set_threads(1);
    assert_eq!(state.threads(), 1);
}

#[test]
fn many_tids_do_not_repeat_in_a_short_run() {
    let state = LibcThreads::new();
    let first = state.next_tid();
    let second = state.next_tid();
    assert_ne!(first, second);
}
