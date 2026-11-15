// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use alloc::string::ToString;

use super::*;

fn args(values: &[&str]) -> alloc::vec::Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn volume_parses() {
    assert_eq!(
        parse_args(&args(&["volume", "50"])).expect("volume"),
        Command::Volume(50)
    );
    assert_eq!(
        parse_args(&args(&["volume", "50%"])).expect("volume"),
        Command::Volume(50)
    );
}

#[test]
fn mute_and_status_parse() {
    assert_eq!(parse_args(&args(&["mute"])).expect("mute"), Command::Mute);
    assert_eq!(
        parse_args(&args(&["status"])).expect("status"),
        Command::Status
    );
}

#[test]
fn a_missing_command_is_usage() {
    assert!(parse_args(&[]).is_err());
}

#[test]
fn an_unknown_command_is_rejected() {
    assert!(parse_args(&args(&["loud"])).is_err());
}

#[test]
fn volume_above_100_is_rejected() {
    assert!(parse_args(&args(&["volume", "101"])).is_err());
}

#[test]
fn a_bad_percentage_is_rejected() {
    assert!(parse_args(&args(&["volume", "half"])).is_err());
}

#[test]
fn extra_arguments_are_rejected() {
    assert!(parse_args(&args(&["mute", "now"])).is_err());
}

#[test]
fn percentages_map_to_q15() {
    assert_eq!(percent_to_gain(0), 0);
    assert_eq!(percent_to_gain(50), 16_384);
    assert_eq!(percent_to_gain(100), 32_768);
    assert_eq!(percent_to_gain(200), 32_768);
}

#[test]
fn the_lines_are_stable() {
    assert_eq!(volume_line(50), "mixer: volume 50% left=16384 right=16384");
    assert_eq!(mute_line(), "mixer: muted left=0 right=0");
    assert_eq!(
        status_line(480, 0, "running"),
        "mixer: status position=480 underruns=0 running"
    );
}
