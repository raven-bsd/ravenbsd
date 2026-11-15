// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Man-page candidate and coverage tests.

use super::*;
use crate::testutil::TempDir;
use std::fs;

const SYMBOLS: &str = r#"
[[symbol]]
name = "setlocale"
ring = 2
area = 2

[[symbol]]
name = "newlocale"
ring = 2
area = 2

[[symbol]]
name = "printf"
ring = 2
area = 3

[[symbol]]
name = "raven_open_scheme"
ring = 1
area = 11

[[symbol]]
name = "gap_sym"
ring = 2
gap = ["PG-1"]

[[symbol]]
name = "warn_sym"
ring = 2
warn = "deprecated"

[[symbol]]
name = "stub_sym"
ring = 2
stub = "ENOSYS"

[[symbol]]
name = "skip_ring3"
ring = 3
"#;

const PAGE: &str = "\
.Dd November 12, 2026
.Dt FIXTURE 3
.Os RavenBSD
.Sh NAME
.Nm setlocale
.Nm newlocale
.Nm printf
.Nm raven_open_scheme
.Nm gap_sym
.Nm warn_sym
.Nm stub_sym
.Nd fixture page
.Sh SYNOPSIS
.In locale.h
.Ft char *
.Fn setlocale \"int category\" \"const char *locale\"
.Sh DESCRIPTION
Fixture description.
.Sh RETURN VALUES
Fixture return values.
.Sh ERRORS
Fixture errors.
.Sh STANDARDS
Fixture standards.
.Sh CAVEATS
Fixture caveats.
.Sh SEE ALSO
.Xr libc 7
";

const LIBC7: &str = "\
.Dd November 12, 2026
.Dt LIBC 7
.Os RavenBSD
.Sh NAME
.Nm libc
.Nd the RavenBSD C library
.Sh SYNOPSIS
Fixture.
.Sh DESCRIPTION
Fixture.
.Sh FILES
Fixture.
.Sh ENVIRONMENT
Fixture.
.Sh STANDARDS
Fixture.
.Sh SEE ALSO
.Xr setlocale 3
";

const PAGES: &str = r#"
[[page]]
name = "fixture"
section = 3
names = ["setlocale", "newlocale", "printf", "raven_open_scheme", "gap_sym", "warn_sym", "stub_sym"]
reasons = ["M4"]
areas = [2, 3, 11]

[[page]]
name = "libc"
section = 7
names = ["libc"]
reasons = ["extra"]
areas = []
"#;

fn fixture() -> TempDir {
    let temp = TempDir::new("man");
    temp.write("libc/ravenlibc/abi/census/symbols.toml", SYMBOLS);
    temp.write("tests/libc/gate/man-pages.toml", PAGES);
    temp.write("docs/man3/fixture.3", PAGE);
    temp.write("docs/man7/libc.7", LIBC7);
    temp
}

fn run_check(temp: &TempDir) -> ManReport {
    check_paths(temp).expect("man check")
}

fn check_paths(temp: &TempDir) -> Result<ManReport> {
    check(
        temp.path(),
        &temp.path().join("libc/ravenlibc/abi/census/symbols.toml"),
        &temp.path().join("tests/libc/gate/man-pages.toml"),
        &temp.path().join("docs"),
    )
}

#[test]
fn selects_m4_m1_m2_and_m3_candidates() {
    let symbols = load_symbols_from(SYMBOLS);
    let selected = candidates(&symbols);
    let names: Vec<&str> = selected
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "gap_sym",
            "newlocale",
            "printf",
            "raven_open_scheme",
            "setlocale",
            "stub_sym",
            "warn_sym"
        ]
    );
}

#[test]
fn ring_three_is_excluded() {
    let symbols = load_symbols_from(SYMBOLS);
    assert!(
        !candidates(&symbols)
            .iter()
            .any(|candidate| candidate.name == "skip_ring3")
    );
}

#[test]
fn the_fixed_m4_list_is_present() {
    assert!(M4.contains(&"setlocale"));
    assert!(M4.contains(&"pthread_create"));
    assert!(M4.contains(&"sysconf"));
}

#[test]
fn candidate_list_is_sorted_and_has_rules() {
    let symbols = load_symbols_from(SYMBOLS);
    let text = list_candidates(&symbols);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 7);
    assert!(lines[0].starts_with("gap_sym\tM2\t"));
    assert!(lines.iter().any(|line| line.starts_with("setlocale\tM4\t")));
}

#[test]
fn the_fixture_passes() {
    let temp = fixture();
    let report = run_check(&temp);
    assert!(report.is_ok(), "{:?}", report.errors);
    assert_eq!(report.pages, 2);
}

#[test]
fn a_missing_page_is_reported() {
    let temp = fixture();
    fs::remove_file(temp.path().join("docs/man3/fixture.3")).unwrap();
    let report = run_check(&temp);
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("missing page"))
    );
}

#[test]
fn a_missing_required_section_is_reported() {
    let temp = fixture();
    let text = PAGE.replace(".Sh ERRORS\nFixture errors.\n", "");
    temp.write("docs/man3/fixture.3", &text);
    let report = run_check(&temp);
    assert!(report.errors.iter().any(|error| error.contains("ERRORS")));
}

#[test]
fn a_missing_nm_alias_is_reported() {
    let temp = fixture();
    let text = PAGE.replace(".Nm newlocale\n", "");
    temp.write("docs/man3/fixture.3", &text);
    let report = run_check(&temp);
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("newlocale"))
    );
}

#[test]
fn a_non_candidate_name_is_reported_without_extra() {
    let temp = fixture();
    let pages = PAGES.replace(
        "names = [\"setlocale\", \"newlocale\", \"printf\", \"raven_open_scheme\", \"gap_sym\", \"warn_sym\", \"stub_sym\"]",
        "names = [\"setlocale\", \"newlocale\", \"printf\", \"raven_open_scheme\", \"gap_sym\", \"warn_sym\", \"stub_sym\", \"bogus\"]",
    );
    let page = PAGE.replace(".Nm stub_sym\n", ".Nm stub_sym\n.Nm bogus\n");
    temp.write("tests/libc/gate/man-pages.toml", &pages);
    temp.write("docs/man3/fixture.3", &page);
    let report = run_check(&temp);
    assert!(report.errors.iter().any(|error| error.contains("bogus")));
}

#[test]
fn an_orphan_page_is_reported() {
    let temp = fixture();
    temp.write("docs/man3/orphan.3", PAGE);
    let report = run_check(&temp);
    assert!(report.errors.iter().any(|error| error.contains("orphan")));
}

#[test]
fn libc7_must_have_its_sections() {
    let temp = fixture();
    let text = LIBC7.replace(".Sh FILES\nFixture.\n", "");
    temp.write("docs/man7/libc.7", &text);
    let report = run_check(&temp);
    assert!(report.errors.iter().any(|error| error.contains("FILES")));
}

#[test]
fn empty_page_manifests_are_rejected() {
    assert!(validate_pages(&[]).is_err());
}

#[test]
fn pages_without_names_are_rejected() {
    assert!(
        validate_pages(&[PageSpec {
            name: "x".to_string(),
            section: 3,
            names: Vec::new(),
            reasons: Vec::new(),
            areas: Vec::new(),
            sources: Vec::new(),
        }])
        .is_err()
    );
}

fn load_symbols_from(text: &str) -> Vec<Symbol> {
    let temp = TempDir::new("man-symbols");
    temp.write("symbols.toml", text);
    load_symbols(&temp.path().join("symbols.toml")).expect("symbols")
}
