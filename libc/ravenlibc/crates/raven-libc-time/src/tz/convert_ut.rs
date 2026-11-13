// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Conversion tests: the donor ATF vectors and the negative-time traps.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::tz::posix_tz::tzparse;

fn utc() -> TzState {
    TzState::utc()
}

#[test]
fn the_epoch_and_its_neighbours() {
    let tm = gmtime_r(0).expect("epoch");
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (70, 0, 1));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (0, 0, 0));
    assert_eq!((tm.tm_wday, tm.tm_yday), (4, 0));

    let tm = gmtime_r(1).expect("one second");
    assert_eq!(tm.tm_sec, 1);

    let tm = gmtime_r(-1).expect("one second before");
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (69, 11, 31));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (23, 59, 59));
    assert_eq!((tm.tm_wday, tm.tm_yday), (3, 364));
}

#[test]
fn the_2038_boundary_and_beyond() {
    let tm = gmtime_r(2_147_483_647).expect("2038");
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (138, 0, 19));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (3, 14, 7));
    assert_eq!((tm.tm_wday, tm.tm_yday), (2, 18));

    let tm = gmtime_r(4_294_967_296).expect("2106");
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (206, 1, 7));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (6, 28, 16));
    assert_eq!((tm.tm_wday, tm.tm_yday), (0, 37));
}

#[test]
fn extreme_values_overflow() {
    assert_eq!(gmtime_r(i64::MAX), Err(errno(EOVERFLOW)));
    assert_eq!(gmtime_r(i64::MIN), Err(errno(EOVERFLOW)));
}

#[test]
fn utc_localtime_matches_gmtime() {
    let state = utc();
    for t in [-1, 0, 1, 1_000_000, 2_147_483_647, 4_294_967_296] {
        let utc_tm = gmtime_r(t).expect("gmtime");
        let local_tm = localtime_r(&state, t).expect("localtime");
        assert_eq!(utc_tm.tm_year, local_tm.tm_year);
        assert_eq!(utc_tm.tm_mon, local_tm.tm_mon);
        assert_eq!(utc_tm.tm_mday, local_tm.tm_mday);
        assert_eq!(utc_tm.tm_hour, local_tm.tm_hour);
        assert_eq!(utc_tm.tm_min, local_tm.tm_min);
        assert_eq!(utc_tm.tm_sec, local_tm.tm_sec);
        assert_eq!(local_tm.tm_isdst, 0);
    }
}

#[test]
fn the_posix_est5edt_rule_selects_standard_and_daylight() {
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).expect("zone");
    let winter = localtime_r(&state, 0).expect("winter");
    assert_eq!(winter.tm_hour, 19);
    assert_eq!(winter.tm_isdst, 0);
    assert_eq!(winter.tm_gmtoff, -5 * 3600);
    assert_eq!(state.abbr(winter.tm_zone), b"EST");

    let summer = localtime_r(&state, 1_700_000_000).expect("summer");
    assert_eq!(summer.tm_isdst, 0); // 2023-11-14 is after the November rule
    let july = localtime_r(&state, 1_689_000_000).expect("july");
    assert_eq!(july.tm_isdst, 1);
    assert_eq!(july.tm_gmtoff, -4 * 3600);
    assert_eq!(state.abbr(july.tm_zone), b"EDT");
}

#[test]
fn timegm_round_trips_gmtime() {
    for t in [
        -2_000_000_000i64,
        -1,
        0,
        1,
        1_000_000,
        2_147_483_647,
        4_294_967_296,
        1_700_000_000,
        -(1i64 << 55),
        1i64 << 55,
    ] {
        let mut tm = gmtime_r(t).expect("gmtime");
        let back = crate::tz::mktime::timegm(&mut tm).expect("timegm");
        assert_eq!(back, t, "t={t}");
    }
}

#[test]
fn offtime_uses_the_requested_offset() {
    let tm = offtime(0, 3600).expect("offtime");
    assert_eq!(tm.tm_hour, 1);
    assert_eq!(tm.tm_gmtoff, 3600);
}

#[test]
fn the_right_zone_conversions_round_trip() {
    let mut state = utc();
    state.leapcnt = 1;
    state.lsis.push(crate::tz::LeapInfo { trans: 0, corr: 1 });
    assert_eq!(time2posix(&state, 1).expect("posix"), 0);
    assert_eq!(posix2time(&state, 0).expect("right"), 1);
}
