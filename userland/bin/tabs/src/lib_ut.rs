// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn default_stops_are_eight_columns() {
    let options = parse_args([]).expect("options");
    assert_eq!(options.style, Style::Stops);
    assert_eq!(options.stops, [8, 16, 24, 32, 40, 48, 56, 64, 72, 80]);
}

#[test]
fn accepts_explicit_stops() {
    let options = parse_args(["4", "12"]).expect("options");
    assert_eq!(options.stops, [4, 12]);
}

#[test]
fn cursor_style_is_selected() {
    let options = parse_args(["-c", "5"]).expect("options");
    assert_eq!(options.style, Style::Cursor);
}

#[test]
fn accepts_terminal_type() {
    let options = parse_args(["-T", "xterm", "6"]).expect("options");
    assert_eq!(options.stops, [6]);
}

#[test]
fn rejects_missing_terminal_value() {
    assert_eq!(parse_args(["-T"]), Err(ParseError::MissingTerminal));
}

#[test]
fn rejects_unsupported_styles() {
    for style in ["-a", "-f", "-p", "-s", "-u", "+m"] {
        assert_eq!(parse_args([style]), Err(ParseError::UnsupportedStyle));
    }
}

#[test]
fn rejects_bad_and_out_of_range_stops() {
    assert_eq!(parse_args(["x"]), Err(ParseError::BadStop));
    assert_eq!(parse_args(["0"]), Err(ParseError::StopRange));
    assert_eq!(parse_args(["1000"]), Err(ParseError::StopRange));
}

#[test]
fn stop_sequence_clears_then_sets_each_stop() {
    let options = Options {
        style: Style::Stops,
        stops: vec![4, 9],
    };
    assert_eq!(render(&options), "\x1b[3g\x1b[4g\x1b[9g");
}

#[test]
fn cursor_sequence_moves_to_each_stop() {
    let options = Options {
        style: Style::Cursor,
        stops: vec![4, 9],
    };
    assert_eq!(render(&options), "\x1b[4G\x1b[9G");
}
