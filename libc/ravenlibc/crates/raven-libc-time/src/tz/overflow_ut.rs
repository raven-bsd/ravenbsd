// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Overflow and leap-year tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn leap_years_follow_the_gregorian_rule() {
    assert!(!isleap(1900));
    assert!(isleap(2000));
    assert!(isleap(2024));
    assert!(!isleap(2100));
    assert!(isleap(-4));
    assert!(!isleap(-1));
}

#[test]
fn isleap_sum_does_not_overflow() {
    let _ = isleap_sum(i64::MAX, i64::MAX);
    let _ = isleap_sum(i64::MAX, i64::MIN);
    assert!(isleap_sum(2000, 4));
}

#[test]
fn year_days_counts_the_leap_day() {
    assert_eq!(year_days(2023), 365);
    assert_eq!(year_days(2024), 366);
    assert_eq!(year_days(1900), 365);
    assert_eq!(year_days(2000), 366);
}

#[test]
fn increment_overflow_stays_inside_i32() {
    let mut value = i32::MAX - 1;
    assert!(!increment_overflow(&mut value, 1));
    assert_eq!(value, i32::MAX);
    assert!(increment_overflow(&mut value, 1));
    let mut value = i32::MIN + 1;
    assert!(!increment_overflow(&mut value, -1));
    assert_eq!(value, i32::MIN);
    assert!(increment_overflow(&mut value, -1));
}

#[test]
fn increment_overflow_time_stays_inside_i64() {
    let mut value = i64::MAX - 1;
    assert!(!increment_overflow_time(&mut value, 1));
    assert_eq!(value, i64::MAX);
    assert!(increment_overflow_time(&mut value, 1));
    let mut value = i64::MIN + 1;
    assert!(!increment_overflow_time(&mut value, -1));
    assert_eq!(value, i64::MIN);
    assert!(increment_overflow_time(&mut value, -1));
}

#[test]
fn checked_add_wrappers_return_none_on_overflow() {
    assert_eq!(ckd_add_i32(1, 2), Some(3));
    assert_eq!(ckd_add_i32(i32::MAX, 1), None);
    assert_eq!(ckd_add_i64(1, 2), Some(3));
    assert_eq!(ckd_add_i64(i64::MAX, 1), None);
}
