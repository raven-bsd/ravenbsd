// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! pjdfstest parser tests.

use raven_conform::pjdfstest::parse;
use raven_conform::{Status, TestResult};

const FIXTURE: &str = include_str!("fixtures/pjdfstest-sample.txt");
const DIRECT: &str = include_str!("fixtures/pjdfstest-direct.txt");
const PROVE_FAIL: &str = include_str!("fixtures/pjdfstest-prove-fail.txt");

#[test]
fn parses_the_direct_fixture() {
    let results = parse(DIRECT, None);
    assert_eq!(results.results.len(), 5);
    assert_eq!(
        status(&results.results, "pjdfstest", "direct pass"),
        Status::Pass
    );
    assert_eq!(
        status(&results.results, "pjdfstest", "direct fail"),
        Status::Fail
    );
    assert_eq!(
        status(&results.results, "pjdfstest", "test 3"),
        Status::Skip
    );
    assert_eq!(
        status(&results.results, "pjdfstest", "test 4"),
        Status::Skip
    );
    assert_eq!(
        status(&results.results, "pjdfstest", "test 5"),
        Status::Skip
    );
}

#[test]
fn parses_the_prove_failure_fixture() {
    let results = parse(PROVE_FAIL, None);
    assert_eq!(results.results.len(), 3);
    assert!(
        results
            .results
            .iter()
            .all(|result| result.suite == "tests/misc/00.t")
    );
    assert_eq!(
        status(&results.results, "tests/misc/00.t", "mismatch"),
        Status::Fail
    );
}

#[test]
fn parses_the_synthetic_fixture() {
    let results = parse(FIXTURE, Some("raven-test"));
    assert_eq!(results.tool, "pjdfstest");
    assert_eq!(results.target.as_deref(), Some("raven-test"));
    assert_eq!(results.results.len(), 10);
    assert_eq!(results.unparsed, 0);

    assert_eq!(
        status(
            &results.results,
            "tests/chmod/00.t",
            "chmod: expected mode 0644"
        ),
        Status::Fail
    );
    assert_eq!(
        status(
            &results.results,
            "tests/chmod/00.t",
            "chmod: expected EACCES"
        ),
        Status::Fail
    );
    assert_eq!(
        status(&results.results, "tests/chmod/00.t", "chmod: symlink"),
        Status::Pass
    );
    assert_eq!(
        status(&results.results, "tests/chmod/00.t", "test 3"),
        Status::Skip
    );
    assert_eq!(
        status(&results.results, "tests/open/00.t", "open O_RDONLY"),
        Status::Pass
    );
}

#[test]
fn plan_and_prove_footer_are_not_unparsed() {
    let results = parse("1..1\nok 1\nResult: PASS\nFiles=1, Tests=1\n", None);
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.unparsed, 0);
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let results = parse("# comment\n\n   \nok 1\n", None);
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.unparsed, 0);
}

#[test]
fn duplicate_names_get_a_suffix() {
    let results = parse("ok 1 - same\nok 2 - same\nnot ok 3 - same\n", None);
    let names: Vec<&str> = results
        .results
        .iter()
        .map(|result| result.name.as_str())
        .collect();
    assert_eq!(names, vec!["same", "same#2", "same#3"]);
}

#[test]
fn unicode_descriptions_round_trip() {
    let results = parse("ok 1 - テスト ✓\n", None);
    assert_eq!(results.results[0].name, "テスト ✓");
    assert_eq!(results.results[0].status, Status::Pass);
}

#[test]
fn skip_and_todo_directives_map_to_skip() {
    let results = parse(
        "ok 1 # skip no root\nok 2 # SKIP not supported\nnot ok 3 # TODO later\n",
        None,
    );
    assert_eq!(results.results.len(), 3);
    assert!(
        results
            .results
            .iter()
            .all(|result| result.status == Status::Skip)
    );
    assert_eq!(results.results[0].message.as_deref(), Some("skip no root"));
}

#[test]
fn prove_summary_assigns_preceding_results_to_the_file() {
    let results = parse(
        "ok 1\nnot ok 2 - boom\ntests/chmod/00.t .. Failed 1/2 subtests\n",
        None,
    );
    assert_eq!(results.results.len(), 2);
    assert!(
        results
            .results
            .iter()
            .all(|result| result.suite == "tests/chmod/00.t")
    );
}

#[test]
fn prove_marker_before_results_assigns_the_suite() {
    let results = parse("tests/open/00.t ..\nok 1\ntests/open/00.t .. ok\n", None);
    assert_eq!(results.results[0].suite, "tests/open/00.t");
}

#[test]
fn direct_output_uses_the_default_suite() {
    let results = parse("ok 1\n", None);
    assert_eq!(results.results[0].suite, "pjdfstest");
}

#[test]
fn truncated_output_keeps_the_parsed_prefix() {
    let results = parse("1..5\nok 1\nnot ok 2 - unfinished\n", None);
    assert_eq!(results.results.len(), 2);
    assert_eq!(results.unparsed, 0);
}

#[test]
fn garbage_lines_are_counted_not_passed() {
    let results = parse("this is not TAP\n{BROKEN\nok 1\n", None);
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.unparsed, 2);
}

#[test]
fn bail_out_is_broken() {
    let results = parse("ok 1\nBail out! cannot continue\n", None);
    assert_eq!(results.results.len(), 2);
    assert_eq!(results.results[0].status, Status::Broken);
    assert_eq!(results.results[0].name, "bail out");
    assert_eq!(results.unparsed, 0);
}

#[test]
fn empty_input_is_valid() {
    let results = parse("", None);
    assert!(results.results.is_empty());
    assert_eq!(results.unparsed, 0);
}

fn status(results: &[TestResult], suite: &str, name: &str) -> Status {
    results
        .iter()
        .find(|result| result.suite == suite && result.name == name)
        .unwrap_or_else(|| panic!("missing {suite}:{name}"))
        .status
}
