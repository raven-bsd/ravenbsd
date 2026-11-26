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

#[test]
fn unlink_removes_a_node_even_when_its_back_link_is_stale() {
    let main = alloc::boxed::Box::new(raven_libc_tls::Pthread::new(0));
    let first = alloc::boxed::Box::new(raven_libc_tls::Pthread::new(0));
    let target = alloc::boxed::Box::new(raven_libc_tls::Pthread::new(0));
    let last = alloc::boxed::Box::new(raven_libc_tls::Pthread::new(0));
    let main_addr = &*main as *const raven_libc_tls::Pthread as usize;
    let first_addr = &*first as *const raven_libc_tls::Pthread as usize;
    let target_addr = &*target as *const raven_libc_tls::Pthread as usize;
    let last_addr = &*last as *const raven_libc_tls::Pthread as usize;

    raw::set_links(&main, last_addr, first_addr);
    raw::set_links(&first, main_addr, target_addr);
    // The target's back link is stale: it points at main, not its real predecessor.
    raw::set_links(&target, main_addr, last_addr);
    raw::set_links(&last, target_addr, main_addr);

    assert!(unlink_from(main_addr, target_addr));
    assert_eq!(raw::next_thread(&main) as *const _ as usize, first_addr);
    assert_eq!(raw::prev_thread(&main) as *const _ as usize, last_addr);
    assert_eq!(raw::next_thread(&first) as *const _ as usize, last_addr);
    assert_eq!(raw::prev_thread(&first) as *const _ as usize, main_addr);
    assert_eq!(raw::next_thread(&last) as *const _ as usize, main_addr);
    assert_eq!(raw::prev_thread(&last) as *const _ as usize, first_addr);
}
