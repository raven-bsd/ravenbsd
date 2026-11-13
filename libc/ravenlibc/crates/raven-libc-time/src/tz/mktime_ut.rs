// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mktime`/`timegm` normalisation and DST tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::tz::posix_tz::tzparse;

fn utc_tm(year: i32, mon: i32, mday: i32, hour: i32, min: i32, sec: i32) -> Tm {
    Tm {
        tm_sec: sec,
        tm_min: min,
        tm_hour: hour,
        tm_mday: mday,
        tm_mon: mon,
        tm_year: year - 1900,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: crate::tz::NO_ZONE,
    }
}

#[test]
fn timegm_epoch_and_neighbours() {
    let mut tm = utc_tm(1970, 0, 1, 0, 0, 0);
    assert_eq!(timegm(&mut tm), Ok(0));
    let mut tm = utc_tm(1970, 0, 1, 0, 0, 1);
    assert_eq!(timegm(&mut tm), Ok(1));
    let mut tm = utc_tm(1969, 11, 31, 23, 59, 59);
    assert_eq!(timegm(&mut tm), Ok(-1));
}

#[test]
fn timegm_negative_year_is_not_the_error_value() {
    let mut tm = utc_tm(1899, 0, 1, 0, 0, 0);
    let value = timegm(&mut tm).expect("negative year");
    assert_ne!(value, -1);
    assert_eq!(tm.tm_year, -1);
}

#[test]
fn seconds_normalise_across_minutes() {
    let mut tm = utc_tm(1970, 0, 1, 0, 0, -1);
    assert_eq!(timegm(&mut tm), Ok(-1));
    let mut tm = utc_tm(1970, 0, 1, 0, 0, 60);
    assert_eq!(timegm(&mut tm), Ok(60));
}

#[test]
fn months_and_days_normalise() {
    let mut tm = utc_tm(1970, 12, 1, 0, 0, 0);
    assert_eq!(timegm(&mut tm), Ok(31_536_000));
    let mut tm = utc_tm(1970, -1, 1, 0, 0, 0);
    assert_eq!(timegm(&mut tm), Ok(-2_678_400));
    let mut tm = utc_tm(1970, 0, 0, 0, 0, 0);
    assert_eq!(timegm(&mut tm), Ok(-86_400));
    let mut tm = utc_tm(1970, 0, 32, 0, 0, 0);
    assert_eq!(timegm(&mut tm), Ok(31 * 86_400));
}

#[test]
fn utc_mktime_matches_timegm() {
    let state = TzState::utc();
    let mut tm = utc_tm(2023, 6, 1, 12, 34, 56);
    let local = mktime(&state, &mut tm).expect("mktime");
    let mut gm = utc_tm(2023, 6, 1, 12, 34, 56);
    let utc = timegm(&mut gm).expect("timegm");
    assert_eq!(local, utc);
}

#[test]
fn est5edt_winter_and_summer() {
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).expect("zone");
    let mut winter = utc_tm(2023, 0, 1, 0, 0, 0);
    winter.tm_isdst = -1;
    assert_eq!(mktime(&state, &mut winter), Ok(1_672_549_200));
    assert_eq!(winter.tm_isdst, 0);
    let mut summer = utc_tm(2023, 6, 1, 0, 0, 0);
    summer.tm_isdst = -1;
    assert_eq!(mktime(&state, &mut summer), Ok(1_688_184_000));
    assert_eq!(summer.tm_isdst, 1);
}

#[test]
fn an_explicit_isdst_uses_the_donor_fallback() {
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).expect("zone");
    let mut tm = utc_tm(1918, 7, 12, 5, -51, 3);
    tm.tm_isdst = 0;
    assert_eq!(mktime(&state, &mut tm), Ok(-1_621_695_057));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (5, 9, 3));
    assert_eq!(tm.tm_isdst, 1);

    let mut tm = utc_tm(1996, 0, 1, 0, 0, 0);
    tm.tm_isdst = 1;
    assert_eq!(mktime(&state, &mut tm), Ok(820_468_800));
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (95, 11, 31));
    assert_eq!(tm.tm_isdst, 0);

    let mut tm = utc_tm(2045, 0, 1, 0, 0, 0);
    tm.tm_isdst = 1;
    assert_eq!(mktime(&state, &mut tm), Ok(2_366_856_000));
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (144, 11, 31));
    assert_eq!(tm.tm_isdst, 0);
}

#[test]
fn negative_seconds_keep_the_donor_remainder() {
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).expect("zone");
    let mut tm = utc_tm(2057, 9, 34, 24, 69, -42);
    tm.tm_isdst = 0;
    assert_eq!(mktime(&state, &mut tm), Ok(2_772_079_698));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (1, 8, 18));
    assert_eq!((tm.tm_mon, tm.tm_mday), (10, 4));
    assert_eq!(tm.tm_isdst, 0);
}

#[test]
fn timeoff_uses_the_offset() {
    let mut tm = utc_tm(1970, 0, 1, 1, 0, 0);
    assert_eq!(timeoff(&mut tm, 3600), Ok(0));
}

#[test]
fn the_spring_gap_and_autumn_overlap() {
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).expect("zone");
    let mut gap = utc_tm(2023, 2, 12, 2, 30, 0);
    gap.tm_isdst = -1;
    // The NetBSD donor's default (no NO_ERROR_IN_DST_GAP) reports EINVAL
    // for a local time that falls in the spring-forward gap.
    assert_eq!(mktime(&state, &mut gap), Err(errno(EINVAL)));

    let mut overlap = utc_tm(2023, 10, 5, 1, 30, 0);
    overlap.tm_isdst = 0;
    assert_eq!(mktime(&state, &mut overlap), Ok(1_699_165_800));
    let mut overlap = utc_tm(2023, 10, 5, 1, 30, 0);
    overlap.tm_isdst = 1;
    assert_eq!(mktime(&state, &mut overlap), Ok(1_699_162_200));
}
