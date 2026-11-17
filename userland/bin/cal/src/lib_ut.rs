// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `cal` Gregorian model.

use super::{
    Options, UsageError, View, civil_from_unix, days_in_month, is_leap, parse_args, render, weekday,
};

fn options(view: View, year: i32, month: u32) -> Options {
    Options { view, year, month }
}

#[test]
fn gregorian_leap_years() {
    assert!(is_leap(2000));
    assert!(is_leap(2024));
    assert!(is_leap(1600));
    assert!(!is_leap(1900));
    assert!(!is_leap(2100));
    assert!(!is_leap(2023));
}

#[test]
fn month_lengths_follow_the_leap_rule() {
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(2023, 2), 28);
    assert_eq!(days_in_month(2026, 1), 31);
    assert_eq!(days_in_month(2026, 4), 30);
    assert_eq!(days_in_month(2026, 12), 31);
}

#[test]
fn known_first_weekdays() {
    assert_eq!(weekday(2026, 3, 1), 0);
    assert_eq!(weekday(2026, 1, 1), 4);
    assert_eq!(weekday(2000, 1, 1), 6);
    assert_eq!(weekday(1970, 1, 1), 4);
}

#[test]
fn unix_epoch_is_january_1970() {
    assert_eq!(civil_from_unix(0), (1970, 1, 1));
    assert_eq!(civil_from_unix(86_399), (1970, 1, 1));
    assert_eq!(civil_from_unix(86_400), (1970, 1, 2));
    assert_eq!(civil_from_unix(951_782_400), (2000, 2, 29));
    assert_eq!(civil_from_unix(-86_400), (1969, 12, 31));
}

#[test]
fn default_month_uses_today() {
    assert_eq!(
        parse_args([], (2026, 11)).unwrap(),
        options(View::Month, 2026, 11)
    );
}

#[test]
fn month_and_year_select_one_month() {
    assert_eq!(
        parse_args(["3", "2026"], (2026, 11)).unwrap(),
        options(View::Month, 2026, 3)
    );
}

#[test]
fn one_operand_is_a_whole_year() {
    assert_eq!(
        parse_args(["2026"], (2026, 11)).unwrap(),
        options(View::Year, 2026, 1)
    );
}

#[test]
fn year_option_defaults_to_today() {
    assert_eq!(
        parse_args(["-y"], (2026, 11)).unwrap(),
        options(View::Year, 2026, 1)
    );
}

#[test]
fn year_option_takes_one_year() {
    assert_eq!(
        parse_args(["-y", "1999"], (2026, 11)).unwrap(),
        options(View::Year, 1999, 1)
    );
}

#[test]
fn three_month_view_defaults_to_today() {
    assert_eq!(
        parse_args(["-3"], (2026, 11)).unwrap(),
        options(View::Three, 2026, 11)
    );
}

#[test]
fn three_month_view_accepts_month_and_year() {
    assert_eq!(
        parse_args(["-3", "3", "2026"], (2026, 11)).unwrap(),
        options(View::Three, 2026, 3)
    );
}

#[test]
fn three_month_view_with_one_operand_is_a_whole_year() {
    assert_eq!(
        parse_args(["-3", "2026"], (2026, 11)).unwrap(),
        options(View::Year, 2026, 1)
    );
}

#[test]
fn double_dash_ends_option_processing() {
    assert_eq!(
        parse_args(["--", "3", "2026"], (2026, 11)).unwrap(),
        options(View::Month, 2026, 3)
    );
}

#[test]
fn invalid_months_are_rejected() {
    assert_eq!(
        parse_args(["0", "2026"], (2026, 11)).unwrap_err(),
        UsageError::BadMonth
    );
    assert_eq!(
        parse_args(["13", "2026"], (2026, 11)).unwrap_err(),
        UsageError::BadMonth
    );
    assert_eq!(
        parse_args(["march", "2026"], (2026, 11)).unwrap_err(),
        UsageError::BadMonth
    );
}

#[test]
fn invalid_years_are_rejected() {
    assert_eq!(
        parse_args(["3", "0"], (2026, 11)).unwrap_err(),
        UsageError::BadYear
    );
    assert_eq!(
        parse_args(["3", "10000"], (2026, 11)).unwrap_err(),
        UsageError::BadYear
    );
    assert_eq!(
        parse_args(["2026x"], (2026, 11)).unwrap_err(),
        UsageError::BadYear
    );
}

#[test]
fn a_three_month_window_needs_both_neighbours() {
    assert_eq!(
        parse_args(["-3", "1", "1"], (2026, 11)).unwrap_err(),
        UsageError::BadYear
    );
    assert_eq!(
        parse_args(["-3", "12", "9999"], (2026, 11)).unwrap_err(),
        UsageError::BadYear
    );
}

#[test]
fn too_many_operands_are_rejected() {
    assert_eq!(
        parse_args(["3", "2026", "x"], (2026, 11)).unwrap_err(),
        UsageError::TooManyOperands
    );
    assert_eq!(
        parse_args(["-y", "3", "2026"], (2026, 11)).unwrap_err(),
        UsageError::TooManyOperands
    );
}

#[test]
fn conflicting_and_unknown_options_are_rejected() {
    assert_eq!(
        parse_args(["-3", "-y"], (2026, 11)).unwrap_err(),
        UsageError::ConflictingOptions
    );
    assert_eq!(
        parse_args(["-x"], (2026, 11)).unwrap_err(),
        UsageError::UnknownOption
    );
}

#[test]
fn march_2026_renders_the_known_grid() {
    let text = render(&options(View::Month, 2026, 3));
    assert_eq!(
        text,
        "     March 2026\nSu Mo Tu We Th Fr Sa\n 1  2  3  4  5  6  7\n 8  9 10 11 12 13 14\n15 16 17 18 19 20 21\n22 23 24 25 26 27 28\n29 30 31\n"
    );
}

#[test]
fn february_2024_renders_the_leap_day() {
    let text = render(&options(View::Month, 2024, 2));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 7);
    assert_eq!(lines[0], "   February 2024");
    assert_eq!(lines[1], "Su Mo Tu We Th Fr Sa");
    assert_eq!(lines[2].trim_start(), "1  2  3");
    assert_eq!(lines[2].len(), 20);
    assert_eq!(lines[3], " 4  5  6  7  8  9 10");
    assert_eq!(lines[4], "11 12 13 14 15 16 17");
    assert_eq!(lines[5], "18 19 20 21 22 23 24");
    assert_eq!(lines[6], "25 26 27 28 29");
}

#[test]
fn three_month_view_shows_both_neighbours() {
    let text = render(&options(View::Three, 2026, 1));
    let first = text.lines().next().unwrap_or("");
    assert!(first.contains("December 2025"), "{first}");
    assert!(first.contains("January 2026"), "{first}");
    assert!(first.contains("February 2026"), "{first}");
}

#[test]
fn three_month_view_wraps_the_year() {
    let text = render(&options(View::Three, 2025, 12));
    let first = text.lines().next().unwrap_or("");
    assert!(first.contains("November 2025"), "{first}");
    assert!(first.contains("December 2025"), "{first}");
    assert!(first.contains("January 2026"), "{first}");
}

#[test]
fn year_view_lists_all_twelve_months() {
    let text = render(&options(View::Year, 2026, 1));
    for name in super::MONTH_NAMES {
        assert!(text.contains(name), "missing {name}");
    }
    let mut lines = text.lines();
    let title = format!("{}2026", " ".repeat(30));
    assert_eq!(lines.next(), Some(title.as_str()));
    assert_eq!(lines.next(), Some(""));
}
