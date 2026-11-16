// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `grep`.

use raven_libc_regex::Regex;

use super::{Report, compile, parse, process};

fn opts(pattern: &str) -> super::Options<'_> {
    super::Options {
        pattern,
        ..super::Options::default()
    }
}

fn compile_ok(options: &super::Options<'_>) -> Regex {
    match compile(options) {
        Ok(regex) => regex,
        Err(_) => panic!("pattern did not compile"),
    }
}

fn process_ok(
    regex: &Regex,
    name: Option<&str>,
    show_name: bool,
    data: &[u8],
    options: &super::Options<'_>,
) -> Report {
    match process(regex, name, show_name, data, options) {
        Ok(report) => report,
        Err(_) => panic!("process failed"),
    }
}

#[test]
fn parse_flags_and_files() {
    let options = parse(["-i", "-n", "-E", "pat", "a", "b"]).unwrap();
    assert!(options.ignore_case);
    assert!(options.line_number);
    assert!(options.extended);
    assert_eq!(options.pattern, "pat");
    assert_eq!(options.files, vec!["a", "b"]);
    assert!(parse(["-x", "pat"]).is_err());
}

#[test]
fn basic_regex_matches_and_reports_lines() {
    let options = opts("b");
    let regex = compile_ok(&options);
    let report = process_ok(&regex, None, false, b"a\nb\nc\n", &options);
    assert!(report.matched);
    assert_eq!(report.count, 1);
    assert_eq!(report.output, vec!["b"]);
}

#[test]
fn extended_and_fixed_modes() {
    let mut options = opts("b+");
    options.extended = true;
    let regex = compile_ok(&options);
    let report = process_ok(&regex, None, false, b"bbb\n", &options);
    assert!(report.matched);

    let mut options = opts("a.b");
    options.fixed = true;
    let regex = compile_ok(&options);
    let report = process_ok(&regex, None, false, b"a.b\naxb\n", &options);
    assert_eq!(report.count, 1);
    assert_eq!(report.output, vec!["a.b"]);
}

#[test]
fn ignore_case_invert_and_number() {
    let mut options = opts("ABC");
    options.ignore_case = true;
    let regex = compile_ok(&options);
    let report = process_ok(&regex, None, false, b"abc\n", &options);
    assert!(report.matched);

    options.invert = true;
    options.line_number = true;
    let regex = compile_ok(&options);
    let report = process_ok(&regex, None, false, b"abc\ndef\n", &options);
    assert_eq!(report.output, vec!["2:def"]);
}

#[test]
fn count_and_file_modes() {
    let mut options = opts("x");
    options.count = true;
    let regex = compile_ok(&options);
    let report = process_ok(&regex, Some("a"), true, b"x\nx\n", &options);
    assert_eq!(report.count, 2);
    assert_eq!(report.output, vec!["a:2"]);

    options.count = false;
    options.files_with_matches = true;
    let regex = compile_ok(&options);
    let report = process_ok(&regex, Some("a"), true, b"x\n", &options);
    assert_eq!(report.output, vec!["a"]);
}

#[test]
fn no_match_is_false() {
    let options = opts("z");
    let regex = compile_ok(&options);
    let report = process_ok(&regex, None, false, b"a\nb\n", &options);
    assert!(!report.matched);
    assert_eq!(report.count, 0);
}
