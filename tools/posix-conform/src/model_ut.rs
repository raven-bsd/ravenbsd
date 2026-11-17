// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for profile validation and traceability rules.

use std::path::{Path, PathBuf};

use crate::model::{
    HeaderRow, InterfaceRow, Meta, Profile, STANDARD, Status, TraceFile, TraceRow, validate,
};
use crate::seed::seed_profile;

fn root(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("posix-conform-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("libc/ravenlibc/abi/census")).unwrap();
    std::fs::create_dir_all(path.join("libc/ravenlibc/include")).unwrap();
    std::fs::create_dir_all(path.join("userland/bin")).unwrap();
    std::fs::write(path.join("libc/ravenlibc/abi/Symbols.list"), "").unwrap();
    path
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

fn meta(id: &str) -> Meta {
    Meta {
        id: id.to_string(),
        standard: STANDARD.to_string(),
        status: Status::Present,
        evidence: "none".to_string(),
        owner: "P1".to_string(),
        waiver: String::new(),
        notes: String::new(),
        pg: Vec::new(),
    }
}

fn empty_profile() -> Profile {
    Profile {
        schema: 1,
        standard: STANDARD.to_string(),
        platform: crate::model::PlatformStatus::new(),
        header: Vec::new(),
        interface: Vec::new(),
        utility: Vec::new(),
        shell_feature: Vec::new(),
        filesystem: Vec::new(),
        process: Vec::new(),
        tty: Vec::new(),
    }
}

fn empty_trace() -> TraceFile {
    TraceFile {
        schema: 1,
        standard: STANDARD.to_string(),
        trace: Vec::new(),
    }
}

#[test]
fn duplicate_ids_are_rejected() {
    let path = root("duplicate");
    let mut profile = empty_profile();
    profile.header.push(HeaderRow {
        meta: meta("same"),
        name: "stdio.h".to_string(),
        required_macros: Vec::new(),
        required_types: Vec::new(),
        required_functions: Vec::new(),
    });
    profile.header.push(HeaderRow {
        meta: meta("same"),
        name: "stdlib.h".to_string(),
        required_macros: Vec::new(),
        required_types: Vec::new(),
        required_functions: Vec::new(),
    });
    let error = validate(&path, &profile, &empty_trace())
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate requirement id"), "{error}");
    cleanup(&path);
}

#[test]
fn unknown_owner_and_missing_evidence_are_rejected() {
    let path = root("fields");
    let mut profile = empty_profile();
    let mut row = meta("interface.foo");
    row.owner = "P9".to_string();
    row.evidence = String::new();
    profile.interface.push(InterfaceRow {
        meta: row,
        name: "foo".to_string(),
        header: "unistd.h".to_string(),
        kind: "func".to_string(),
        required: true,
        signature: String::new(),
    });
    let error = validate(&path, &profile, &empty_trace())
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown owner"), "{error}");
    assert!(error.contains("missing evidence"), "{error}");
    cleanup(&path);
}

#[test]
fn waived_without_reason_is_rejected() {
    let path = root("waived");
    let mut profile = empty_profile();
    let mut row = meta("process.fork");
    row.status = Status::Waived;
    row.waiver = String::new();
    profile.process.push(crate::model::BehaviorRow {
        meta: row,
        name: "fork".to_string(),
        description: "fork".to_string(),
    });
    let error = validate(&path, &profile, &empty_trace())
        .unwrap_err()
        .to_string();
    assert!(error.contains("waived without a reason"), "{error}");
    cleanup(&path);
}

#[test]
fn trace_coverage_is_required() {
    let path = root("trace");
    let profile = empty_profile();
    let trace = TraceFile {
        schema: 1,
        standard: STANDARD.to_string(),
        trace: vec![TraceRow {
            id: "orphan".to_string(),
            ticket: "P1".to_string(),
            test: "none".to_string(),
            evidence: "none".to_string(),
        }],
    };
    let error = validate(&path, &profile, &trace).unwrap_err().to_string();
    assert!(error.contains("has no profile row"), "{error}");
    cleanup(&path);
}

#[test]
fn current_export_must_be_classified() {
    let path = root("export");
    std::fs::write(
        path.join("libc/ravenlibc/abi/census/symbols.toml"),
        "[[symbol]]\nname = \"foo\"\nring = 0\nkind = \"func\"\nstandard = [\"posix\"]\n",
    )
    .unwrap();
    std::fs::write(path.join("libc/ravenlibc/abi/Symbols.list"), "foo\n").unwrap();
    let error = validate(&path, &empty_profile(), &empty_trace())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("current export `foo` is not classified"),
        "{error}"
    );
    cleanup(&path);
}

#[test]
fn a_utility_source_directory_does_not_imply_present() {
    let path = root("utility-status");
    std::fs::create_dir(path.join("userland/bin/cat")).unwrap();
    let mut profile = empty_profile();
    let mut row = meta("util.cat");
    row.status = Status::Missing;
    profile.utility.push(crate::model::UtilityRow {
        meta: row,
        name: "cat".to_string(),
        options: String::new(),
        exit_status: String::new(),
        locale: String::new(),
        test: "none".to_string(),
    });
    let trace = TraceFile {
        schema: 1,
        standard: STANDARD.to_string(),
        trace: vec![TraceRow {
            id: "util.cat".to_string(),
            ticket: "P4".to_string(),
            test: "none".to_string(),
            evidence: "none".to_string(),
        }],
    };
    // The source directory is not executable evidence; the evidence overlays own the status.
    validate(&path, &profile, &trace).unwrap();
    cleanup(&path);
}

#[test]
fn present_utility_evidence_must_exist() {
    let path = root("utility-evidence");
    let mut profile = empty_profile();
    let mut row = meta("util.cat");
    row.status = Status::Present;
    row.evidence = "test:tests/posix/utility/cat/test.sh".to_string();
    profile.utility.push(crate::model::UtilityRow {
        meta: row,
        name: "cat".to_string(),
        options: String::new(),
        exit_status: String::new(),
        locale: String::new(),
        test: "none".to_string(),
    });
    let error = validate(&path, &profile, &empty_trace())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("evidence `test:tests/posix/utility/cat/test.sh` is absent"),
        "{error}"
    );
    cleanup(&path);
}

#[test]
fn probes_hidden_in_posix_are_collected() {
    let path = root("hidden");
    std::fs::create_dir_all(path.join("tests/libc/headers")).unwrap();
    std::fs::write(
        path.join("tests/libc/headers/probes.toml"),
        "[[probe]]\nsymbol = \"fgetln\"\nhidden_in = [\"posix\", \"gnu\"]\n",
    )
    .unwrap();
    let hidden = crate::seed::posix_hidden_symbols(&path);
    assert!(hidden.contains("fgetln"), "{hidden:?}");
    cleanup(&path);
}

#[test]
fn posix_tag_filter_accepts_posix_and_rejects_bsd_only() {
    assert!(crate::model::is_posix_relevant(&["posix".to_string()]));
    assert!(crate::model::is_posix_relevant(&[]));
    assert!(!crate::model::is_posix_relevant(&["bsd".to_string()]));
    assert!(!crate::model::is_posix_relevant(&["iso-c23".to_string()]));
}

#[test]
fn seed_profile_is_deterministic_and_large_enough() {
    let path = root("seed");
    std::fs::write(
        path.join("libc/ravenlibc/abi/census/symbols.toml"),
        "[[symbol]]\nname = \"foo\"\nring = 0\nkind = \"func\"\nstandard = [\"posix\"]\n",
    )
    .unwrap();
    let first = seed_profile(&path).unwrap();
    let second = seed_profile(&path).unwrap();
    assert_eq!(first.header, second.header);
    assert_eq!(first.interface, second.interface);
    assert!(first.utility.len() > 100);
    assert!(!first.utility.is_empty());
    cleanup(&path);
}
