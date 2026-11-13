// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_fresh_list_lock_is_unowned() {
    let _serial = crate::test_support::serial();
    reset_for_fork_child();
    assert_eq!(owner(), 0);
}

#[test]
fn locking_records_the_owner_tid() {
    let _serial = crate::test_support::serial();
    reset_for_fork_child();
    let kernel = crate::test_support::MockKernel::new();
    crate::libc_state::LIBC.set_need_locks(0);
    lock(&kernel);
    assert_eq!(owner(), 100);
    unlock(&kernel);
    assert_eq!(owner(), 0);
}

#[test]
fn the_owner_word_is_the_tid_and_a_single_unlock_clears_it() {
    let _serial = crate::test_support::serial();
    reset_for_fork_child();
    let kernel = crate::test_support::MockKernel::new();
    lock(&kernel);
    assert_eq!(owner(), 100);
    // The crate never takes the list lock recursively (create/exit/join/key_delete take it once),
    // so the lock intentionally has no depth counter; a second `lock` by the owner returns
    // without changing the word and the single `unlock` releases it.
    lock(&kernel);
    unlock(&kernel);
    assert_eq!(owner(), 0);
}

#[test]
fn sync_returns_when_the_lock_is_free() {
    let _serial = crate::test_support::serial();
    reset_for_fork_child();
    let kernel = crate::test_support::MockKernel::new();
    sync(&kernel);
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn the_fork_child_resets_the_lock() {
    let _serial = crate::test_support::serial();
    reset_for_fork_child();
    let kernel = crate::test_support::MockKernel::new();
    lock(&kernel);
    reset_for_fork_child();
    assert_eq!(owner(), 0);
}
