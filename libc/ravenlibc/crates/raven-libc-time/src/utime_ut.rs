// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `utime` conversion tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn a_missing_buffer_means_now() {
    let now = Timespec {
        tv_sec: 10,
        tv_nsec: 20,
    };
    assert_eq!(utime_timespecs(None, now), [now, now]);
}

#[test]
fn an_explicit_buffer_is_microseconds() {
    let pair = utime_timespecs(Some((7, 123_456)), Timespec::ZERO);
    assert_eq!(pair[0].tv_sec, 7);
    assert_eq!(pair[0].tv_nsec, 123_456_000);
    assert_eq!(pair[0], pair[1]);
}

#[test]
fn a_zero_microsecond_field_is_zero_nanoseconds() {
    let pair = utime_timespecs(Some((7, 0)), Timespec::ZERO);
    assert_eq!(pair[0].tv_nsec, 0);
}
