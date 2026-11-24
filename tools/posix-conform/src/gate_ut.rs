// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the published gate report.

use std::path::Path;

use crate::gate::{GateReport, Verdict, probe_source, render, repository_commit};
use crate::model::{
    BehaviorRow, InterfaceRow, Meta, PlatformStatus, Profile, STANDARD, Status, TraceFile,
};
use crate::render::render_gaps;

fn report() -> GateReport {
    GateReport {
        schema: 1,
        profile: "posix".to_string(),
        arch: "x86_64".to_string(),
        host_only: true,
        date: "2026-11-15T00:00:00Z".to_string(),
        commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
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
    assert!(text.contains("- Commit: `0123456789abcdef0123456789abcdef01234567`"));
}

#[test]
fn explicit_commit_wins_without_touching_git() {
    let commit = repository_commit(Path::new("/nonexistent"), Some("  abc123  "));
    assert_eq!(commit, "abc123");
}

#[test]
fn missing_git_checkout_records_unknown() {
    let commit = repository_commit(Path::new("/nonexistent"), None);
    assert_eq!(commit, "unknown");
}

#[test]
fn every_verdict_is_spelled_in_lowercase() {
    assert_eq!(Verdict::Pass.as_str(), "pass");
    assert_eq!(Verdict::Fail.as_str(), "fail");
    assert_eq!(Verdict::Skip.as_str(), "skip");
}

#[test]
fn gaps_keep_a_waiver_visible() {
    let profile = Profile {
        schema: 1,
        standard: STANDARD.to_string(),
        platform: PlatformStatus::new(),
        header: Vec::new(),
        interface: Vec::new(),
        utility: Vec::new(),
        shell_feature: Vec::new(),
        filesystem: Vec::new(),
        process: vec![BehaviorRow {
            meta: Meta {
                id: "process.fork".to_string(),
                standard: STANDARD.to_string(),
                status: Status::Waived,
                evidence: "none".to_string(),
                owner: "P5".to_string(),
                waiver: "OWNER WAIVER: P5 omits fork for the first gate".to_string(),
                notes: String::new(),
                pg: Vec::new(),
            },
            name: "fork".to_string(),
            description: "fork".to_string(),
        }],
        tty: Vec::new(),
    };
    let trace = TraceFile {
        schema: 1,
        standard: STANDARD.to_string(),
        trace: Vec::new(),
    };
    let text = render_gaps(Path::new("."), &profile, &trace).unwrap();
    assert!(text.contains("waived"), "{text}");
    assert!(
        text.contains("waiver: OWNER WAIVER: P5 omits fork for the first gate"),
        "{text}"
    );
}

fn interface_row(name: &str, header: &str) -> InterfaceRow {
    InterfaceRow {
        meta: Meta {
            id: format!("interface.{name}"),
            standard: STANDARD.to_string(),
            status: Status::Present,
            evidence: "none".to_string(),
            owner: "P1".to_string(),
            waiver: String::new(),
            notes: String::new(),
            pg: Vec::new(),
        },
        name: name.to_string(),
        header: header.to_string(),
        kind: "func".to_string(),
        required: true,
        signature: String::new(),
    }
}

#[test]
fn lockf_probe_uses_the_posix_header() {
    let source = probe_source(&interface_row("lockf", "sys/stat.h"));
    assert!(source.contains("#include <unistd.h>"), "{source}");
    assert!(source.contains("&lockf"), "{source}");
}

#[test]
fn seed_header_gaps_use_the_posix_header() {
    for (name, seed_header, posix_header) in [
        ("statvfs", "math.h", "sys/statvfs.h"),
        ("strtof", "math.h", "stdlib.h"),
        ("wcsftime", "time.h", "wchar.h"),
        ("wcstoimax", "wchar.h", "inttypes.h"),
        ("wcstoumax", "wchar.h", "inttypes.h"),
    ] {
        let source = probe_source(&interface_row(name, seed_header));
        assert!(
            source.contains(&format!("#include <{posix_header}>")),
            "{name}: {source}"
        );
        assert!(source.contains(&format!("&{name}")), "{name}: {source}");
    }
}

#[test]
fn macro_rows_use_a_macro_probe_not_an_address() {
    for (name, header, needle) in [
        (
            "offsetof",
            "stddef.h",
            "offsetof(struct probe_object, member)",
        ),
        ("va_start", "stdarg.h", "va_start(ap, count)"),
        ("va_arg", "stdarg.h", "va_arg(ap, int)"),
        ("va_copy", "stdarg.h", "va_copy(copy, ap)"),
        ("va_end", "stdarg.h", "va_end(ap)"),
    ] {
        let source = probe_source(&interface_row(name, header));
        assert!(source.contains(needle), "{name}: {source}");
        assert!(!source.contains(&format!("&{name}")), "{name}: {source}");
    }
}
