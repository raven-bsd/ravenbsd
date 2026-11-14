// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::{Case, Outcome, Verdict};

fn outcome(case: &str, verdict: Verdict, reason: &str) -> Outcome {
    Outcome::new(Case::new("s", "p", case), verdict, reason, "C40")
}

#[test]
fn tap_lists_every_body_once() {
    let text = render(
        &[
            outcome("a", Verdict::Pass, ""),
            outcome("b", Verdict::Skip, "no network"),
            outcome("c", Verdict::Fail, "bad"),
        ],
        Format::Tap,
    );
    assert!(text.starts_with("1..3\n"));
    assert!(text.contains("ok 1 - s/p/a"));
    assert!(text.contains("ok 2 - s/p/b # SKIP no network"));
    assert!(text.contains("not ok 3 - s/p/c # FAIL bad"));
}

#[test]
fn tap_marks_not_run_as_skip() {
    let text = render(
        &[outcome("a", Verdict::NotRun, "no-run-backend")],
        Format::Tap,
    );
    assert!(text.contains("# SKIP not-run no-run-backend"));
}

#[test]
fn json_is_a_valid_array_shape() {
    let text = render(
        &[
            outcome("a", Verdict::Pass, ""),
            outcome("b", Verdict::Fail, "bad"),
        ],
        Format::Json,
    );
    assert!(text.starts_with("[\n"));
    assert!(text.ends_with("]\n"));
    assert!(text.contains("\"case\":\"a\""));
    assert!(text.contains("\"verdict\":\"fail\""));
    assert!(text.contains("\"reason\":\"bad\""));
}

#[test]
fn json_escapes_quotes_and_controls() {
    let text = render(
        &[outcome("a\"b", Verdict::Fail, "line\nnext")],
        Format::Json,
    );
    assert!(text.contains("a\\\"b"));
    assert!(text.contains("line\\nnext"));
}

#[test]
fn junit_counts_failures_and_skips() {
    let text = render(
        &[
            outcome("a", Verdict::Pass, ""),
            outcome("b", Verdict::Fail, "bad"),
            outcome("c", Verdict::Skip, "no network"),
        ],
        Format::Junit,
    );
    assert!(text.contains("tests=\"3\" failures=\"1\" skipped=\"1\""));
    assert!(text.contains("<testcase name=\"s/p/a\"/>"));
    assert!(text.contains("<failure message=\"bad\"/>"));
}

#[test]
fn junit_escapes_xml() {
    let text = render(&[outcome("a", Verdict::Fail, "<bad>")], Format::Junit);
    assert!(text.contains("&lt;bad&gt;"));
}

#[test]
fn human_is_tab_separated() {
    let text = render(&[outcome("a", Verdict::Pass, "")], Format::Human);
    assert_eq!(text, "s\tp\ta\tpass\t\tC40\n");
}

#[test]
fn formats_parse_by_name() {
    assert_eq!(Format::parse("tap"), Some(Format::Tap));
    assert_eq!(Format::parse("json"), Some(Format::Json));
    assert_eq!(Format::parse("junit"), Some(Format::Junit));
    assert_eq!(Format::parse("human"), Some(Format::Human));
    assert_eq!(Format::parse("xml"), None);
}

#[test]
fn an_empty_report_is_still_well_formed() {
    assert_eq!(render(&[], Format::Tap), "1..0\n");
    assert_eq!(render(&[], Format::Json), "[\n]\n");
    assert!(render(&[], Format::Junit).contains("tests=\"0\""));
}
