// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `strptime` vectors, including the donor's ATF cases.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::locale::CLocale;

fn blank() -> Tm {
    Tm {
        tm_sec: -1,
        tm_min: -1,
        tm_hour: -1,
        tm_mday: -1,
        tm_mon: -1,
        tm_year: -1,
        tm_wday: -1,
        tm_yday: -1,
        tm_isdst: -1,
        tm_gmtoff: -1,
        tm_zone: crate::tz::NO_ZONE,
    }
}

fn ctx() -> StrptimeCtx<'static> {
    StrptimeCtx {
        tzname: [b"UTC", b"UTC"],
        localtime: None,
    }
}

fn parse(buf: &str, fmt: &str) -> Option<(Tm, usize)> {
    let state = TzState::utc();
    let mut tm = blank();
    let rest = strptime(
        &state,
        &CLocale,
        buf.as_bytes(),
        fmt.as_bytes(),
        &mut tm,
        &ctx(),
    )?;
    Some((tm, buf.len() - rest.len()))
}

#[test]
fn the_common_atf_case() {
    let (tm, used) = parse("Tue Jan 20 23:27:46 1998", "%a %b %d %T %Y").expect("common");
    assert_eq!(used, 24);
    assert_eq!((tm.tm_sec, tm.tm_min, tm.tm_hour), (46, 27, 23));
    assert_eq!((tm.tm_mday, tm.tm_mon, tm.tm_year), (20, 0, 98));
    assert_eq!((tm.tm_wday, tm.tm_yday), (2, 19));
}

#[test]
fn the_donor_c_locale_case() {
    let (tm, used) = parse("Fri Mar  4 20:05:34 2005", "%c").expect("c");
    assert_eq!(used, 24);
    assert_eq!((tm.tm_sec, tm.tm_min, tm.tm_hour), (34, 5, 20));
    assert_eq!((tm.tm_mday, tm.tm_mon, tm.tm_year), (4, 2, 105));
    assert_eq!((tm.tm_wday, tm.tm_yday), (5, 62));
}

#[test]
fn whitespace_in_the_format_eats_a_run() {
    let (tm, used) = parse("5\t3  4 8pm:05:34 2005", "%w%n%m%t%d%n%k%p:%M:%S %Y").expect("ws");
    assert_eq!(used, 21);
    assert_eq!((tm.tm_mday, tm.tm_mon, tm.tm_year), (4, 2, 105));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (20, 5, 34));
}

#[test]
fn day_names() {
    for (text, expected) in [
        ("Sun", 0),
        ("Sunday", 0),
        ("Mon", 1),
        ("tueSDay", 2),
        ("SaturDay", 6),
    ] {
        let (tm, _) = parse(text, "%A").expect("day");
        assert_eq!(tm.tm_wday, expected);
    }
    assert!(parse("Moon", "%A").is_none());
    assert!(parse("sunday", "%EA").is_none());
}

#[test]
fn month_names() {
    for (text, expected) in [
        ("Jan", 0),
        ("January", 0),
        ("september", 8),
        ("septembe", 8),
        ("Mayor", 4),
        ("Mars", 2),
    ] {
        let (tm, _) = parse(text, "%b").expect("month");
        assert_eq!(tm.tm_mon, expected);
    }
    assert!(parse("Rover", "%b").is_none());
}

#[test]
fn the_hour_and_second_ranges() {
    assert!(parse("00", "%I").is_none());
    assert!(parse("13", "%I").is_none());
    assert!(parse("24", "%H").is_none());
    assert_eq!(parse("23", "%H").expect("hour").0.tm_hour, 23);
    assert_eq!(parse("61", "%S").expect("second").0.tm_sec, 61);
    assert!(parse("62", "%S").is_none());
}

#[test]
fn century_and_year_combinations() {
    let (tm, _) = parse("x20y", "x%Cy").expect("C");
    assert_eq!(tm.tm_year, 100);
    let (tm, _) = parse("x84y", "x%yy").expect("y");
    assert_eq!(tm.tm_year, 84);
    let (tm, _) = parse("x2084y", "x%C%yy").expect("Cy");
    assert_eq!(tm.tm_year, 184);
    let (tm, _) = parse("x8420y", "x%y%Cy").expect("yC");
    assert_eq!(tm.tm_year, 184);
}

#[test]
fn the_iso_date_and_time_formats() {
    let (tm, used) = parse("1998-01-20", "%F").expect("F");
    assert_eq!(used, 10);
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (98, 0, 20));
    assert_eq!(tm.tm_yday, 19);
    let (tm, used) = parse("01/20/98", "%D").expect("D");
    assert_eq!(used, 8);
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (98, 0, 20));
    let (tm, used) = parse("23:27:46", "%T").expect("T");
    assert_eq!(used, 8);
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (23, 27, 46));
    let (tm, used) = parse("23:27", "%R").expect("R");
    assert_eq!(used, 5);
    assert_eq!((tm.tm_hour, tm.tm_min), (23, 27));
}

#[test]
fn the_epoch_seconds_case() {
    let (tm, used) = parse("0", "%s").expect("epoch");
    assert_eq!(used, 1);
    assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday), (70, 0, 1));
    assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (0, 0, 0));
}

#[test]
fn a_huge_epoch_seconds_value_fails() {
    assert!(parse("999999999999999999999999", "%s").is_none());
}

#[test]
fn numeric_zones() {
    for (text, offset) in [
        ("Z", 0),
        ("GMT", 0),
        ("UT", 0),
        ("UTC", 0),
        ("+03", 3 * 3600),
        ("-03", -3 * 3600),
        ("+0403", 4 * 3600 + 3 * 60),
        ("-04:03", -(4 * 3600 + 3 * 60)),
        ("+14:00", 14 * 3600),
        ("-23:59", -(23 * 3600 + 59 * 60)),
        ("EST", -5 * 3600),
        ("EDT", -4 * 3600),
        ("PST", -8 * 3600),
    ] {
        let (tm, _) = parse(text, "%z").expect(text);
        assert_eq!(tm.tm_gmtoff, offset, "{text}");
    }
    for text in [
        "1", "03", "0304", "+1", "-203", "+12345", "+2400", "-2400", "+1060",
    ] {
        assert!(parse(text, "%z").is_none(), "{text}");
    }
}

#[test]
fn named_zones_use_the_context() {
    let (tm, used) = parse("UTC", "%Z").expect("UTC");
    assert_eq!(used, 3);
    assert_eq!(tm.tm_isdst, 0);
    let (tm, used) = parse("EST", "%Z").expect("EST");
    assert_eq!(used, 3);
    assert_eq!(tm.tm_gmtoff, -5 * 3600);
}

#[test]
fn the_week_of_year_is_finalised() {
    let (tm, _) = parse("2023 01", "%Y %U").expect("week");
    assert!(tm.tm_yday >= 0);
    assert!(tm.tm_wday >= 0);
    assert!(tm.tm_mday >= 1);
}

#[test]
fn the_day_of_year_is_converted() {
    let (tm, _) = parse("2023 060", "%Y %j").expect("yday");
    assert_eq!(tm.tm_yday, 59);
    assert_eq!((tm.tm_mon, tm.tm_mday), (2, 1));
}

#[test]
fn partial_parse_returns_the_tail() {
    let state = TzState::utc();
    let mut tm = blank();
    let rest =
        strptime(&state, &CLocale, b"2023-07-01tail", b"%F", &mut tm, &ctx()).expect("partial");
    assert_eq!(rest, b"tail");
}

#[test]
fn a_literal_mismatch_fails() {
    assert!(parse("23:27", "%H-%M").is_none());
}

#[test]
fn fields_not_named_by_the_format_keep_their_value() {
    let state = TzState::utc();
    let mut tm = blank();
    tm.tm_gmtoff = 1234;
    tm.tm_zone = crate::tz::ZoneIdx(7);
    let rest = strptime(&state, &CLocale, b"23", b"%H", &mut tm, &ctx()).expect("hour");
    assert!(rest.is_empty());
    assert_eq!(tm.tm_gmtoff, 1234);
    assert_eq!(tm.tm_zone, crate::tz::ZoneIdx(7));
}

#[test]
fn strftime_strptime_round_trip() {
    let state = TzState::utc();
    let original = Tm {
        tm_sec: 56,
        tm_min: 34,
        tm_hour: 12,
        tm_mday: 1,
        tm_mon: 6,
        tm_year: 123,
        tm_wday: 6,
        tm_yday: 181,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: crate::tz::NO_ZONE,
    };
    let formats = ["%Y-%m-%dT%H:%M:%S%z", "%a, %d %b %Y %H:%M:%S %Z", "%s"];
    for format in formats {
        let mut buf = [0u8; 128];
        let length = crate::strftime::strftime(
            &state,
            &CLocale,
            &mut buf,
            format.as_bytes(),
            &original,
            &crate::strftime::StrftimeCtx {
                tzname: [b"UTC", b"UTC"],
                localtime: None,
            },
        );
        assert!(length > 0, "{format}");
        let text = core::str::from_utf8(buf.get(..length).unwrap_or(b"")).expect("utf8");
        let mut parsed = blank();
        let rest = strptime(
            &state,
            &CLocale,
            text.as_bytes(),
            format.as_bytes(),
            &mut parsed,
            &ctx(),
        )
        .unwrap_or_else(|| panic!("parse {format}"));
        assert!(rest.is_empty(), "{format}");
        assert_eq!(parsed.tm_year, original.tm_year, "{format}");
        assert_eq!(parsed.tm_mon, original.tm_mon, "{format}");
        assert_eq!(parsed.tm_mday, original.tm_mday, "{format}");
        assert_eq!(parsed.tm_hour, original.tm_hour, "{format}");
        assert_eq!(parsed.tm_min, original.tm_min, "{format}");
        assert_eq!(parsed.tm_sec, original.tm_sec, "{format}");
    }
}
