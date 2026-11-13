// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `strftime` conversion and buffer-size tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::locale::CLocale;

fn epoch() -> Tm {
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

fn summer() -> Tm {
    Tm {
        tm_sec: 56,
        tm_min: 34,
        tm_hour: 12,
        tm_mday: 1,
        tm_mon: 6,
        tm_year: 123,
        tm_wday: 6,
        tm_yday: 181,
        tm_isdst: 1,
        tm_gmtoff: -4 * 3600,
        tm_zone: crate::tz::NO_ZONE,
    }
}

fn format(tm: &Tm, spec: &str) -> Vec<u8> {
    let state = TzState::utc();
    let mut out = [0u8; 128];
    let length = strftime(
        &state,
        &CLocale,
        &mut out,
        spec.as_bytes(),
        tm,
        &StrftimeCtx {
            tzname: [b"UTC", b"EDT"],
            localtime: None,
        },
    );
    out.get(..length).unwrap_or(b"").to_vec()
}

#[test]
fn the_epoch_conversions() {
    let tm = epoch();
    assert_eq!(format(&tm, "%Y"), b"1970");
    assert_eq!(format(&tm, "%y"), b"70");
    assert_eq!(format(&tm, "%C"), b"19");
    assert_eq!(format(&tm, "%m"), b"01");
    assert_eq!(format(&tm, "%d"), b"01");
    assert_eq!(format(&tm, "%e"), b" 1");
    assert_eq!(format(&tm, "%H"), b"00");
    assert_eq!(format(&tm, "%I"), b"12");
    assert_eq!(format(&tm, "%M"), b"00");
    assert_eq!(format(&tm, "%S"), b"00");
    assert_eq!(format(&tm, "%j"), b"001");
    assert_eq!(format(&tm, "%w"), b"4");
    assert_eq!(format(&tm, "%u"), b"4");
    assert_eq!(format(&tm, "%U"), b"00");
    assert_eq!(format(&tm, "%W"), b"00");
    assert_eq!(format(&tm, "%V"), b"01");
    assert_eq!(format(&tm, "%G"), b"1970");
    assert_eq!(format(&tm, "%g"), b"70");
}

#[test]
fn the_epoch_names_and_compound_formats() {
    let tm = epoch();
    assert_eq!(format(&tm, "%a"), b"Thu");
    assert_eq!(format(&tm, "%A"), b"Thursday");
    assert_eq!(format(&tm, "%b"), b"Jan");
    assert_eq!(format(&tm, "%B"), b"January");
    assert_eq!(format(&tm, "%h"), b"Jan");
    assert_eq!(format(&tm, "%p"), b"AM");
    assert_eq!(format(&tm, "%P"), b"am");
    assert_eq!(format(&tm, "%c"), b"Thu Jan  1 00:00:00 1970");
    assert_eq!(format(&tm, "%x"), b"01/01/70");
    assert_eq!(format(&tm, "%X"), b"00:00:00");
    assert_eq!(format(&tm, "%r"), b"12:00:00 AM");
    assert_eq!(format(&tm, "%D"), b"01/01/70");
    assert_eq!(format(&tm, "%F"), b"1970-01-01");
    assert_eq!(format(&tm, "%R"), b"00:00");
    assert_eq!(format(&tm, "%T"), b"00:00:00");
    assert_eq!(format(&tm, "%v"), b" 1-Jan-1970");
    assert_eq!(format(&tm, "%Z"), b"UTC");
    assert_eq!(format(&tm, "%z"), b"+0000");
    assert_eq!(format(&tm, "%s"), b"0");
    assert_eq!(format(&tm, "%n"), b"\n");
    assert_eq!(format(&tm, "%t"), b"\t");
    assert_eq!(format(&tm, "%%"), b"%");
}

#[test]
fn padding_flags() {
    let tm = epoch();
    assert_eq!(format(&tm, "%-d"), b"1");
    assert_eq!(format(&tm, "%_d"), b" 1");
    assert_eq!(format(&tm, "%0d"), b"01");
    assert_eq!(format(&tm, "%-j"), b"1");
    assert_eq!(format(&tm, "%_j"), b"  1");
    assert_eq!(format(&tm, "%0j"), b"001");
    assert_eq!(format(&tm, "%-m"), b"1");
    assert_eq!(format(&tm, "%_m"), b" 1");
}

#[test]
fn case_flags() {
    let tm = epoch();
    assert_eq!(format(&tm, "%^a"), b"THU");
    assert_eq!(format(&tm, "%#p"), b"am");
    assert_eq!(format(&tm, "%#Z"), b"UTC");
}

#[test]
fn summer_offsets_and_names() {
    let tm = summer();
    assert_eq!(format(&tm, "%Y-%m-%dT%H:%M:%S"), b"2023-07-01T12:34:56");
    assert_eq!(format(&tm, "%z"), b"-0400");
    assert_eq!(format(&tm, "%Z"), b"EDT");
    assert_eq!(format(&tm, "%I %p"), b"12 PM");
}

#[test]
fn an_unknown_conversion_is_copied() {
    let tm = epoch();
    assert_eq!(format(&tm, "%Q"), b"%Q");
    assert_eq!(format(&tm, "%"), b"%");
}

#[test]
fn the_buffer_must_hold_the_nul() {
    let state = TzState::utc();
    let tm = epoch();
    let ctx = StrftimeCtx::default();
    let mut out = [0u8; 25];
    assert_eq!(strftime(&state, &CLocale, &mut out, b"%c", &tm, &ctx), 24);
    let mut small = [0u8; 24];
    assert_eq!(strftime(&state, &CLocale, &mut small, b"%c", &tm, &ctx), 0);
}

#[test]
fn empty_output_is_zero() {
    let state = TzState::utc();
    let tm = epoch();
    let ctx = StrftimeCtx::default();
    let mut out = [0u8; 1];
    assert_eq!(strftime(&state, &CLocale, &mut out, b"", &tm, &ctx), 0);
}

#[test]
fn percent_s_clears_a_daylight_saving_flag() {
    let tm = Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 1,
        tm_mon: 6,
        tm_year: 123,
        tm_wday: 6,
        tm_yday: 181,
        tm_isdst: 1,
        tm_gmtoff: -4 * 3600,
        tm_zone: crate::tz::NO_ZONE,
    };
    assert_eq!(format(&tm, "%s"), b"1688184000");
}

#[test]
fn bad_weekday_and_month_use_question_marks() {
    let mut tm = epoch();
    tm.tm_wday = 9;
    tm.tm_mon = 12;
    assert_eq!(format(&tm, "%a %b"), b"? ?");
}

#[test]
fn iso_week_year_boundaries() {
    let mut tm = epoch();
    tm.tm_year = 121;
    tm.tm_mon = 0;
    tm.tm_mday = 1;
    tm.tm_wday = 5;
    tm.tm_yday = 0;
    assert_eq!(format(&tm, "%G-%V"), b"2020-53");
    let mut tm = epoch();
    tm.tm_year = 120;
    tm.tm_mon = 11;
    tm.tm_mday = 31;
    tm.tm_wday = 4;
    tm.tm_yday = 365;
    assert_eq!(format(&tm, "%G-%V"), b"2020-53");
    let mut tm = epoch();
    tm.tm_year = 122;
    tm.tm_mon = 0;
    tm.tm_mday = 1;
    tm.tm_wday = 6;
    tm.tm_yday = 0;
    assert_eq!(format(&tm, "%G-%V"), b"2021-52");
}

#[test]
fn week_numbers_use_the_sunday_and_monday_origins() {
    let mut tm = epoch();
    tm.tm_wday = 4;
    tm.tm_yday = 0;
    assert_eq!(format(&tm, "%U %W"), b"00 00");
    tm.tm_wday = 0;
    tm.tm_yday = 7;
    assert_eq!(format(&tm, "%U %W"), b"02 01");
}
