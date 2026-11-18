// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the alarm deadline model.

use super::Alarm;

#[test]
fn a_new_alarm_is_unarmed() {
    let alarm = Alarm::new();
    assert!(!alarm.active());
    assert_eq!(alarm.remaining_ns(100), 0);
    assert_eq!(alarm.interval_ns(), 0);
}

#[test]
fn alarm_reports_the_previous_remaining_time() {
    let mut alarm = Alarm::new();
    assert_eq!(alarm.set(0, 5_000_000_000), 0);
    assert!(alarm.active());
    assert_eq!(alarm.remaining_ns(2_000_000_000), 3_000_000_000);
    assert_eq!(alarm.set(2_000_000_000, 1_000_000_000), 3_000_000_000);
    assert_eq!(alarm.remaining_ns(2_500_000_000), 500_000_000);
}

#[test]
fn zero_cancels_and_returns_the_remainder() {
    let mut alarm = Alarm::new();
    alarm.set(0, 10_000_000_000);
    assert_eq!(alarm.set(4_000_000_000, 0), 6_000_000_000);
    assert!(!alarm.active());
    assert_eq!(alarm.remaining_ns(4_000_000_000), 0);
}

#[test]
fn an_interval_is_kept_and_a_oneshot_clears_it() {
    let mut alarm = Alarm::new();
    alarm.set_interval(0, 100_000_000, 50_000_000);
    assert!(alarm.active());
    assert_eq!(alarm.interval_ns(), 50_000_000);
    alarm.set(1_000_000, 2_000_000);
    assert_eq!(alarm.interval_ns(), 0);
}

#[test]
fn the_deadline_saturates() {
    let mut alarm = Alarm::new();
    alarm.set(u64::MAX - 1, 10);
    assert_eq!(alarm.deadline_ns(), u64::MAX);
    assert_eq!(alarm.remaining_ns(u64::MAX), 0);
}
