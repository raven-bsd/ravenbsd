// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K274 fixture-oracle tests: the pf.conf subset parses and prints the expected
//! `pfctl -nv` form, and malformed input fails with a bounded, line-numbered error.

use std::fs;
use std::path::PathBuf;

use raven_pfctl::{FileLoader, parse_config, parse_config_with_loader, print_config};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/pfctl")
}

fn read(name: &str) -> String {
    fs::read_to_string(fixture_dir().join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

struct IncludeLoader;

impl FileLoader for IncludeLoader {
    fn read(&self, path: &str) -> Result<String, String> {
        fs::read_to_string(fixture_dir().join("includes").join(path))
            .map_err(|error| error.to_string())
    }
}

#[test]
fn config_fixture_prints_the_expected_output() {
    let config = parse_config(&read("pf.conf")).expect("pf.conf parses");
    assert_eq!(print_config(&config), read("pf.conf.stdout"));
}

#[test]
fn printed_config_round_trips_through_the_parser() {
    let printed = print_config(&parse_config(&read("pf.conf")).expect("parse"));
    let reparsed = parse_config(&printed).expect("printed output parses");
    assert_eq!(print_config(&reparsed), printed);
}

#[test]
fn includes_expand_in_place() {
    let text = read("includes/main.conf");
    let config =
        parse_config_with_loader(&text, "main.conf", &IncludeLoader).expect("include expansion");
    assert_eq!(print_config(&config), read("includes/main.conf.stdout"));
}

#[test]
fn invalid_fixtures_fail_with_line_numbers() {
    let mut checked = 0;
    for entry in fs::read_dir(fixture_dir().join("invalid")).expect("invalid dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("conf") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .expect("name");
        let expected = fs::read_to_string(path.with_extension("conf.err"))
            .unwrap_or_else(|error| panic!("{name}.err: {error}"));
        let mut fields = expected.trim().splitn(2, '|');
        let line: u32 = fields.next().expect("line").parse().expect("line number");
        let needle = fields.next().expect("substring");
        let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        let failure = match parse_config(&text) {
            Ok(_) => panic!("{name} must fail"),
            Err(error) => error,
        };
        assert_eq!(failure.line, line, "{name}: {}", failure.display());
        assert!(
            failure.message.contains(needle),
            "{name}: expected `{needle}' in `{}'",
            failure.message
        );
        checked += 1;
    }
    assert!(checked >= 5, "expected at least five invalid fixtures");
}

#[test]
fn a_malformed_config_never_panics() {
    // Negative/bounds check: truncated suffixes of a valid config must all return
    // Err (or a partial Config) and never panic.
    let text = read("pf.conf");
    for cut in 0..text.len() {
        let prefix = &text[..cut];
        let _ = parse_config(prefix);
    }
}
