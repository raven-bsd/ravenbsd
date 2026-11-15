// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the published gate report.

use crate::gate::{GateReport, Verdict, render};

fn report() -> GateReport {
    GateReport {
        schema: 1,
        profile: "posix".to_string(),
        arch: "x86_64".to_string(),
        host_only: true,
        date: "2026-11-15T00:00:00Z".to_string(),
        tool: "posix-conform test".to_string(),
        rows: vec![
            crate::gate::ResultRow {
                id: "header.stdio.h".to_string(),
                area: "header".to_string(),
                result: Verdict::Pass,
                command: "cc probe".to_string(),
                reason: String::new(),
            },
            crate::gate::ResultRow {
                id: "utility.awk".to_string(),
                area: "utility".to_string(),
                result: Verdict::Fail,
                command: "profile".to_string(),
                reason: "missing".to_string(),
            },
            crate::gate::ResultRow {
                id: "external.pjdfstest".to_string(),
                area: "external".to_string(),
                result: Verdict::Skip,
                command: "PJDFSTEST=<path>".to_string(),
                reason: "not set".to_string(),
            },
        ],
        pass: 1,
        fail: 1,
        skip: 1,
        release_blocked: true,
    }
}

#[test]
fn report_has_the_contract_header_and_counts() {
    let text = render(&report());
    assert!(text.contains("POSIX.1-2024 self-certification"));
    assert!(text.contains("| header | 1 | 0 | 0 |"));
    assert!(text.contains("| utility | 0 | 1 | 0 |"));
    assert!(text.contains("| external | 0 | 0 | 1 |"));
    assert!(text.contains("FAIL (release blocked)"));
    assert!(text.contains("2026-11-15T00:00:00Z"));
}

#[test]
fn every_verdict_is_spelled_in_lowercase() {
    assert_eq!(Verdict::Pass.as_str(), "pass");
    assert_eq!(Verdict::Fail.as_str(), "fail");
    assert_eq!(Verdict::Skip.as_str(), "skip");
}
