// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use alloc::string::ToString;

use super::*;

fn args(values: &[&str]) -> alloc::vec::Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn a_single_path_parses() {
    let options = parse_args(&args(&["/tmp/tone.wav"])).expect("options");
    assert_eq!(options.path, "/tmp/tone.wav");
}

#[test]
fn no_path_is_usage() {
    let error = parse_args(&[]).expect_err("usage");
    assert!(error.contains("usage"));
}

#[test]
fn two_paths_are_usage() {
    let error = parse_args(&args(&["a.wav", "b.wav"])).expect_err("usage");
    assert!(error.contains("usage"));
}

#[test]
fn an_option_is_rejected() {
    let error = parse_args(&args(&["--fast", "a.wav"])).expect_err("option");
    assert!(error.contains("unknown option"));
}

#[test]
fn help_is_usage() {
    assert!(parse_args(&args(&["--help"])).is_err());
}

#[test]
fn the_summary_names_the_format() {
    assert_eq!(
        summary(48_000, 48_000, 2),
        "play: ok 48000 frames, 48000 Hz, 2 channels"
    );
    assert_eq!(
        summary(100, 44_100, 1),
        "play: ok 100 frames, 44100 Hz, 1 channel"
    );
}
