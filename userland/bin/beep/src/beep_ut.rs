// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use alloc::string::ToString;

use super::*;

fn args(values: &[&str]) -> alloc::vec::Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn the_defaults_are_440_hz_and_one_second() {
    let options = parse_args(&[]).expect("options");
    assert_eq!(options, Options::default());
    assert_eq!(summary(&options), "beep: ok 440 Hz 1000 ms");
}

#[test]
fn both_options_parse() {
    let options =
        parse_args(&args(&["--frequency", "880", "--duration-ms", "250"])).expect("options");
    assert_eq!(options.frequency_hz, 880);
    assert_eq!(options.duration_ms, 250);
}

#[test]
fn short_options_parse() {
    let options = parse_args(&args(&["-f", "523", "-d", "100"])).expect("options");
    assert_eq!(options.frequency_hz, 523);
    assert_eq!(options.duration_ms, 100);
}

#[test]
fn zero_frequency_is_rejected() {
    assert!(parse_args(&args(&["-f", "0"])).is_err());
}

#[test]
fn a_frequency_above_nyquist_is_rejected() {
    assert!(parse_args(&args(&["-f", "20001"])).is_err());
}

#[test]
fn zero_duration_is_rejected() {
    assert!(parse_args(&args(&["-d", "0"])).is_err());
}

#[test]
fn a_long_duration_is_rejected() {
    assert!(parse_args(&args(&["-d", "60001"])).is_err());
}

#[test]
fn a_bad_number_is_rejected() {
    assert!(parse_args(&args(&["-f", "loud"])).is_err());
}

#[test]
fn a_missing_value_is_rejected() {
    assert!(parse_args(&args(&["-f"])).is_err());
}

#[test]
fn an_unknown_argument_is_rejected() {
    assert!(parse_args(&args(&["--loud"])).is_err());
}
