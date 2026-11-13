// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `TZ` parser vectors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

fn utoff(name: &[u8]) -> i32 {
    tzparse(name, None).expect("tzparse").ttis[0].utoff
}

#[test]
fn utc_and_fixed_offsets() {
    let state = tzparse(b"UTC0", None).expect("UTC0");
    assert_eq!(state.typecnt, 1);
    assert_eq!(state.ttis[0].utoff, 0);
    assert_eq!(state.chars, b"UTC\0");
    assert_eq!(utoff(b"EST5"), -5 * 3600);
    assert_eq!(utoff(b"JST-9"), 9 * 3600);
    assert_eq!(utoff(b"UTC-5"), 5 * 3600);
    assert_eq!(utoff(b"<+03>-3"), 3 * 3600);
    assert_eq!(utoff(b"<+0530>-5:30"), 5 * 3600 + 30 * 60);
}

#[test]
fn est5edt_gets_the_builtin_us_rule() {
    let state = tzparse(b"EST5EDT", None).expect("EST5EDT");
    assert_eq!(state.typecnt, 2);
    assert_eq!(state.ttis[0].utoff, -5 * 3600);
    assert_eq!(state.ttis[1].utoff, -4 * 3600);
    assert!(state.timecnt >= 2);
    assert!(state.goahead);
}

#[test]
fn explicit_rules_and_offsets() {
    for name in [
        b"PST8PDT,M3.2.0,M11.1.0".as_slice(),
        b"CET-1CEST,M3.5.0,M10.5.0/3".as_slice(),
        b"<-03>3<-02>,M3.5.0/-2,M10.5.0/-1".as_slice(),
        b"EST5EDT4,M3.2.0,M11.1.0".as_slice(),
        b"EST5EDT,M3.2.0/26:00,M11.1.0/2:00".as_slice(),
        b"EST5EDT,J60,J300".as_slice(),
        b"EST5EDT,59,300".as_slice(),
    ] {
        let state = tzparse(name, None).expect("valid TZ string");
        assert_eq!(state.typecnt, 2, "{}", String::from_utf8_lossy(name));
        assert!(state.timecnt > 0);
    }
}

#[test]
fn default_dst_offset_is_one_hour_before_standard() {
    let state = tzparse(b"EST5EDT", None).expect("EST5EDT");
    assert_eq!(state.ttis[1].utoff - state.ttis[0].utoff, 3600);
}

#[test]
fn malformed_strings_are_rejected() {
    for name in [
        b"".as_slice(),
        b"A".as_slice(),
        b"<>".as_slice(),
        b"EST".as_slice(),
        b"EST5EDT,,".as_slice(),
        b"EST5EDT,M13.1.0,M11.1.0".as_slice(),
        b"EST5EDT,M3.6.0,M11.1.0".as_slice(),
        b"EST5EDT,M3.1.7,M11.1.0".as_slice(),
        b"EST5EDT,M3.2.0".as_slice(),
        b"EST5EDT,M3.2.0,".as_slice(),
        b"EST5EDT,,".as_slice(),
    ] {
        assert!(
            tzparse(name, None).is_err(),
            "{}",
            String::from_utf8_lossy(name)
        );
    }
}

#[test]
fn julian_day_february_29_is_a_rule() {
    let state = tzparse(b"EST5EDT,J60,J300", None).expect("J rules");
    assert!(state.timecnt > 0);
    let leap = tzparse(b"EST5EDT,60,300", None).expect("n rules");
    assert!(leap.timecnt > 0);
}

#[test]
fn the_footer_base_sets_the_transition_window() {
    let base = tzparse(b"UTC0", None).expect("base");
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", Some(&base)).expect("footer");
    assert_eq!(state.typecnt, 2);
    assert!(state.timecnt > 0);
}

#[test]
fn a_long_name_is_rejected() {
    let mut name = alloc::vec::Vec::new();
    name.push(b'<');
    name.extend(core::iter::repeat_n(b'A', 300));
    name.push(b'>');
    name.extend_from_slice(b"0");
    assert!(tzparse(&name, None).is_err());
}

#[test]
fn a_valid_angle_name_is_accepted() {
    let state = tzparse(b"<+05:30>-5:30", None).expect("angle");
    assert_eq!(state.ttis[0].utoff, 5 * 3600 + 30 * 60);
    assert_eq!(state.chars, b"+05:30\0");
}

#[test]
fn a_rule_time_can_be_negative() {
    let state = tzparse(b"EST5EDT,M3.2.0/-2,M11.1.0/-1", None).expect("negative rule time");
    assert_eq!(state.typecnt, 2);
}

#[test]
fn a_rule_time_can_exceed_a_day() {
    let state = tzparse(b"EST5EDT,M3.2.0/167:59:59,M11.1.0/2:00", None).expect("large rule time");
    assert_eq!(state.typecnt, 2);
}
