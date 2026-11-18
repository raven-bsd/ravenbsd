// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn defaults_to_ufs2_block0() {
    let options = parse_args([]).expect("options");
    assert_eq!(options.device, "block:0");
    assert_eq!(options.version, 2);
    assert_eq!(options.block_size, 4096);
    assert_eq!(options.fragment_size, 512);
    assert_eq!(options.size, 0);
}

#[test]
fn parses_geometry_and_label() {
    let options = parse_args([
        "-O", "1", "-b", "8192", "-f", "1024", "-s", "64m", "-L", "scratch", "block:0",
    ])
    .expect("options");
    assert_eq!(options.version, 1);
    assert_eq!(options.block_size, 8192);
    assert_eq!(options.fragment_size, 1024);
    assert_eq!(options.size, 64 * 1024 * 1024);
    assert_eq!(options.label, "scratch");
}

#[test]
fn rejects_bad_version_and_geometry() {
    assert_eq!(parse_args(["-O", "3"]), Err(ParseError::BadVersion));
    assert_eq!(parse_args(["-b", "5000"]), Err(ParseError::BadGeometry));
    assert_eq!(
        parse_args(["-f", "2048", "-b", "1024"]),
        Err(ParseError::BadGeometry)
    );
    assert_eq!(parse_args(["-s", "1000"]), Err(ParseError::BadGeometry));
}

#[test]
fn rejects_unknown_and_missing_values() {
    assert_eq!(parse_args(["-x"]), Err(ParseError::UnknownOption));
    assert_eq!(parse_args(["-b"]), Err(ParseError::MissingValue));
    assert_eq!(parse_args(["-L"]), Err(ParseError::MissingValue));
}

#[test]
fn summary_is_one_line() {
    let options = parse_args(["-L", "x"]).expect("options");
    assert_eq!(
        summary(&options),
        "newfs: block:0 UFS2 block=4096 frag=512 size=0 label=x"
    );
}
