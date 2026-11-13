// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the link-time macros.

#![allow(clippy::indexing_slicing)]

use core::sync::atomic::{AtomicUsize, Ordering};

static INIT_CALLS: AtomicUsize = AtomicUsize::new(0);

pub fn dummy_hook(value: usize) -> usize {
    value
}

unsafe fn strong_hook(value: usize) -> usize {
    value + 100
}

crate::weak_hook! {
    /// Test hook installed through the function-pointer cell.
    fn test_hook(value: usize) -> usize;
    dummy = super::dummy_hook;
}

crate::lazy! {
    static LAZY_VALUE: usize = {
        INIT_CALLS.fetch_add(1, Ordering::Relaxed);
        77
    };
}

pub fn original_function() -> u32 {
    5
}

crate::alias! {
    /// Alias used by the test.
    pub alias_function = original_function;
}

crate::unimplemented_symbol!(stub_no_args, gap = "G-99", errno = 38);
crate::unimplemented_symbol!(
    stub_with_args,
    fn stub_with_args(value: i32) -> i32,
    gap = "G-98",
    errno = 38
);

#[test]
fn weak_hook_uses_the_dummy_without_an_install() {
    test_hook::install(None);
    assert_eq!(test_hook(7), 7);
}

#[test]
fn weak_hook_prefers_the_installed_strong_function() {
    test_hook::install(Some(strong_hook));
    assert_eq!(test_hook(7), 107);
    test_hook::install(None);
    assert_eq!(test_hook(7), 7);
}

#[test]
fn lazy_initialises_exactly_once() {
    INIT_CALLS.store(0, Ordering::Relaxed);
    assert_eq!(*LAZY_VALUE::get(), 77);
    assert_eq!(*LAZY_VALUE::get(), 77);
    assert_eq!(INIT_CALLS.load(Ordering::Relaxed), 1);
}

#[test]
fn alias_resolves_to_the_target() {
    assert_eq!(alias_function(), 5);
}

#[test]
fn unimplemented_symbol_returns_minus_one() {
    assert_eq!(stub_no_args(), -1);
    assert_eq!(stub_with_args(3), -1);
}

#[test]
fn unimplemented_symbol_records_name_and_gap() {
    assert_eq!(stub_no_args::RECORD.name_str(), "stub_no_args");
    assert_eq!(stub_no_args::RECORD.gap_str(), "G-99");
    assert_eq!(stub_with_args::RECORD.name_str(), "stub_with_args");
    assert_eq!(stub_with_args::RECORD.gap_str(), "G-98");
}
