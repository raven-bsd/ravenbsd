// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::AtomicI32;

fn armed() -> core::sync::atomic::AtomicI32 {
    LIBC.set_need_locks(1);
    AtomicI32::new(0)
}

#[test]
fn a_single_threaded_process_skips_the_lock() {
    let _serial = crate::test_support::serial();
    LIBC.set_need_locks(0);
    let word = AtomicI32::new(0);
    let kernel = crate::test_support::MockKernel::new();
    lock(&kernel, &word);
    assert_eq!(word.load(Ordering::Acquire), 0);
    unlock(&kernel, &word);
    assert_eq!(word.load(Ordering::Acquire), 0);
}

#[test]
fn the_fast_path_takes_the_lock() {
    let _serial = crate::test_support::serial();
    let word = armed();
    let kernel = crate::test_support::MockKernel::new();
    lock(&kernel, &word);
    assert_eq!(word.load(Ordering::Acquire), i32::MIN + 1);
    unlock(&kernel, &word);
    assert_eq!(word.load(Ordering::Acquire), 0);
}

#[test]
fn unlock_without_congestion_does_not_wake() {
    let _serial = crate::test_support::serial();
    let word = armed();
    let kernel = crate::test_support::MockKernel::new();
    lock(&kernel, &word);
    unlock(&kernel, &word);
    assert_eq!(kernel.wake_calls(), 0);
}

#[test]
fn congestion_two_is_left_positive_by_unlock() {
    let _serial = crate::test_support::serial();
    let word = AtomicI32::new(i32::MIN + 2);
    let kernel = crate::test_support::MockKernel::new();
    unlock(&kernel, &word);
    assert_eq!(word.load(Ordering::Acquire), 1);
}

#[test]
fn a_contended_unlock_wakes_one() {
    let _serial = crate::test_support::serial();
    let word = AtomicI32::new(i32::MIN + 2);
    let kernel = crate::test_support::MockKernel::new();
    unlock(&kernel, &word);
    assert_eq!(kernel.wake_calls(), 1);
}

#[test]
fn threaded_reports_the_process_flag() {
    let _serial = crate::test_support::serial();
    LIBC.set_need_locks(1);
    assert!(threaded());
    LIBC.set_need_locks(0);
    assert!(!threaded());
}

#[test]
fn a_negative_need_locks_is_reset_by_the_next_lock() {
    let _serial = crate::test_support::serial();
    LIBC.set_need_locks(-1);
    let word = AtomicI32::new(0);
    let kernel = crate::test_support::MockKernel::new();
    lock(&kernel, &word);
    assert_eq!(LIBC.need_locks(), 0);
    assert_eq!(word.load(Ordering::Acquire), i32::MIN + 1);
    LIBC.set_need_locks(0);
}

#[test]
fn word_helpers_use_the_given_index() {
    let _serial = crate::test_support::serial();
    LIBC.set_need_locks(1);
    let words = [0u32; 4];
    let kernel = crate::test_support::MockKernel::new();
    lock_word(&kernel, &words, 2);
    assert_eq!(
        crate::raw::at_u32(&words, 2).load(Ordering::Acquire),
        (i32::MIN + 1) as u32
    );
    unlock_word(&kernel, &words, 2);
    assert_eq!(crate::raw::at_u32(&words, 2).load(Ordering::Acquire), 0);
    assert_eq!(words[0], 0);
    LIBC.set_need_locks(0);
}

#[test]
fn wait_on_returns_immediately_when_the_word_differs() {
    let _serial = crate::test_support::serial();
    let word = AtomicI32::new(7);
    let kernel = crate::test_support::MockKernel::new();
    wait_on(&kernel, &word, 0);
    assert_eq!(kernel.wait_calls(), 0);
}
