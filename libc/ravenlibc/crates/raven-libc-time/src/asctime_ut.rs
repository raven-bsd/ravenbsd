// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `asctime_r` tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

fn tm() -> Tm {
    Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 1,
        tm_mon: 0,
        tm_year: 70,
        tm_wday: 4,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: crate::tz::NO_ZONE,
    }
}

fn text(out: &[u8; 26]) -> &[u8] {
    let end = out.iter().position(|byte| *byte == 0).unwrap_or(out.len());
    out.get(..end).unwrap_or(b"")
}

#[test]
fn the_epoch_is_the_classic_line() {
    let mut out = [0u8; 26];
    asctime_r(&tm(), &mut out).expect("asctime");
    assert_eq!(text(&out), b"Thu Jan  1 00:00:00 1970\n");
}

#[test]
fn a_long_year_overflows() {
    let mut value = tm();
    value.tm_year = 10000 - 1900;
    let mut out = [0u8; 26];
    assert_eq!(asctime_r(&value, &mut out), Err(errno(EOVERFLOW)));
}

#[test]
fn bad_weekday_and_month_become_questions() {
    let mut value = tm();
    value.tm_wday = 7;
    value.tm_mon = 12;
    let mut out = [0u8; 26];
    asctime_r(&value, &mut out).expect("asctime");
    assert_eq!(text(&out), b"??? ???  1 00:00:00 1970\n");
}

#[test]
fn the_checked_variant_reports_none() {
    let mut out = [0u8; 26];
    assert_eq!(asctime_checked(None, &mut out), Err(errno(EINVAL)));
    assert_eq!(text(&out), b"??? ??? ?? ??:??:?? ????\n");
}
