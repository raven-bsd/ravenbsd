// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn ranks_are_ordered_outermost_first() {
    assert!(core::hint::black_box(rank::ATFORK) < rank::LOADER);
    assert!(core::hint::black_box(rank::LOADER) < rank::KEYS);
    assert!(core::hint::black_box(rank::KEYS) < rank::PTC);
    assert!(core::hint::black_box(rank::PTC) < rank::SUBSYSTEM);
    assert!(core::hint::black_box(rank::SUBSYSTEM) < rank::FILE);
    assert!(core::hint::black_box(rank::FILE) < rank::MALLOC);
    assert!(core::hint::black_box(rank::MALLOC) < rank::KILL);
    assert!(core::hint::black_box(rank::KILL) < rank::THREAD_LIST);
}

#[test]
fn a_fresh_guard_holds_nothing() {
    let guard = RankGuard::new();
    assert_eq!(guard.current(), None);
}

#[test]
fn taking_locks_in_order_is_accepted() {
    let mut guard = RankGuard::new();
    assert_eq!(guard.push(rank::PTC), None);
    assert_eq!(guard.push(rank::KILL), Some(rank::PTC));
    assert_eq!(guard.push(rank::THREAD_LIST), Some(rank::KILL));
    assert_eq!(guard.current(), Some(rank::THREAD_LIST));
}

#[test]
fn an_inversion_is_rejected() {
    let mut guard = RankGuard::new();
    guard.push(rank::THREAD_LIST);
    assert_eq!(guard.push(rank::PTC), None);
}

#[test]
fn popping_restores_the_previous_rank() {
    let mut guard = RankGuard::new();
    guard.push(rank::KEYS);
    guard.push(rank::PTC);
    guard.pop();
    assert_eq!(guard.current(), Some(rank::KEYS));
    guard.pop();
    assert_eq!(guard.current(), None);
}

#[test]
fn popping_an_empty_guard_is_harmless() {
    let mut guard = RankGuard::new();
    guard.pop();
    guard.pop();
    assert_eq!(guard.current(), None);
}

#[test]
fn equal_ranks_are_allowed() {
    let mut guard = RankGuard::new();
    guard.push(rank::SUBSYSTEM);
    assert_eq!(guard.push(rank::SUBSYSTEM), Some(rank::SUBSYSTEM));
}

#[test]
fn the_guard_defaults_to_empty() {
    let guard = RankGuard::default();
    assert_eq!(guard.current(), None);
}
