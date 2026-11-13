// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the engine's state sets, `stepback` and the back-reference budget.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::consts::{BACKREF_BUDGET, BACKREF_DEPTH_MAX, REG_ESPACE, REG_EXTENDED};
use crate::locale::{ByteLocale, Utf8Locale};
use crate::{RegError, RegMatch, regcomp};

#[test]
fn small_and_large_sets_agree_on_a_script() {
    let mut small = SmallSet::new(16);
    let mut large = LargeSet::new(16);
    for index in [0usize, 3, 7, 15] {
        small.set1(index);
        large.set1(index);
    }
    for index in 0..16 {
        assert_eq!(small.is_set(index), large.is_set(index));
    }
    let mut small_backup = small.clone();
    let mut large_backup = large.clone();
    small_backup.fwd_self(0, 4);
    large_backup.fwd_self(0, 4);
    assert_eq!(small_backup.is_set(4), large_backup.is_set(4));
    small_backup.back_self(7, 3);
    large_backup.back_self(7, 3);
    assert_eq!(small_backup.is_set(4), large_backup.is_set(4));
    assert!(small_backup.issetback(7, 3));
    assert!(large_backup.issetback(7, 3));
    assert!(!small_backup.issetback(1, 3));
    assert!(!large_backup.issetback(1, 3));
}

#[test]
fn sets_clear_assign_and_compare() {
    let mut small = SmallSet::new(8);
    small.set1(2);
    let mut other = SmallSet::new(8);
    assert!(!small.eq(&other));
    other.assign(&small);
    assert!(small.eq(&other));
    other.clear();
    assert!(other.is_empty());
}

#[test]
fn large_set_bounds_do_not_panic() {
    let mut set = LargeSet::new(4);
    set.set1(1000);
    assert!(!set.is_set(1000));
    set.fwd_self(1000, 1);
    set.fwd_self(0, 1000);
    set.back_self(0, 1000);
    assert!(!set.is_state_in(1000));
}

#[test]
fn stepback_walks_utf8_characters() {
    let subject = "\u{e9}\u{20ac}x".as_bytes();
    assert_eq!(stepback(&Utf8Locale, subject, 0, 5, 1), Some(2));
    assert_eq!(stepback(&Utf8Locale, subject, 0, 5, 2), Some(0));
    assert_eq!(stepback(&Utf8Locale, subject, 0, 5, 3), None);
    assert_eq!(stepback(&Utf8Locale, subject, 0, 0, 1), None);
}

#[test]
fn stepback_byte_mode_matches_the_donor_off_by_one() {
    assert_eq!(stepback(&ByteLocale, b"abc", 0, 3, 1), Some(2));
    assert_eq!(stepback(&ByteLocale, b"abc", 0, 2, 2), None);
    assert_eq!(stepback(&ByteLocale, b"abc", 1, 3, 1), Some(2));
}

#[test]
fn the_backref_logical_frame_is_small() {
    // The explicit stack frame of `backref` (H-RE-2). The machine stack itself is constant
    // depth, so BACKREF_DEPTH_MAX cannot overflow a small thread stack.
    let frame = core::mem::size_of::<BackrefFrame>();
    let call = core::mem::size_of::<BackrefCall>();
    println!("backref logical frame: {frame} bytes (call {call} bytes)");
    assert!(frame <= 64);
    assert!(call <= 64);
}

#[test]
fn a_tiny_budget_turns_a_backref_pattern_into_espase() {
    let re = regcomp(b"\\(a*\\)*\\1", 0, &ByteLocale).expect("compile");
    let mut subject = vec![b'a'; 30];
    subject.push(b'b');
    let mut slots: [RegMatch; 0] = [];
    let result = crate::regexec::regexec_limited(&re, &subject, &mut slots, 0, &ByteLocale, 8);
    assert_eq!(result, Err(RegError(REG_ESPACE)));
}

#[test]
fn the_linear_path_never_touches_the_budget() {
    let re = regcomp(b"(a*)*b", REG_EXTENDED, &ByteLocale).expect("compile");
    let mut slots: [RegMatch; 0] = [];
    let subject = vec![b'a'; 10_000];
    let result = crate::regexec::regexec_limited(&re, &subject, &mut slots, 0, &ByteLocale, 1);
    assert_eq!(result, Err(RegError(crate::consts::REG_NOMATCH)));
}

#[test]
fn the_hardening_constants_are_the_ticket_values() {
    assert_eq!(BACKREF_BUDGET, 1 << 24);
    assert_eq!(BACKREF_DEPTH_MAX, 4096);
}
