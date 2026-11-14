// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::{BuildStatus, Framework};

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-suite-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

fn minimal() -> SuiteFile {
    SuiteFile {
        schema: 1,
        suite: "netbsd-libc".to_string(),
        donor: "netbsd".to_string(),
        framework: Framework::Atf,
        root: "vendor/netbsd/libc".to_string(),
        srcdir_for_tests: true,
        shell_tests: vec!["string/t_shell.sh".to_string()],
        build: None,
        prebuilt: None,
        programs: vec![Program {
            name: "string/t_strchr".to_string(),
            sources: vec!["string/t_strchr.c".to_string()],
            cflags: Vec::new(),
            ldadd: Vec::new(),
            status: "ok".to_string(),
            status_reason: None,
            expect_exit: 0,
            args: Vec::new(),
        }],
    }
}

#[test]
fn a_minimal_suite_round_trips() {
    let root = temp("roundtrip");
    let path = root.join("suite.toml");
    fs::write(
        &path,
        render_suite(&minimal(), "2026-11-14").expect("render"),
    )
    .expect("write");
    let loaded = load(&path).expect("load");
    assert_eq!(loaded.suite, "netbsd-libc");
    assert_eq!(loaded.programs.len(), 1);
    assert!(loaded.srcdir_for_tests);
}

#[test]
fn a_bad_schema_is_rejected() {
    let mut suite = minimal();
    suite.schema = 2;
    assert!(validate(&suite).is_err());
}

#[test]
fn a_bad_suite_name_is_rejected() {
    let mut suite = minimal();
    suite.suite = "NetBSD_Libc".to_string();
    assert!(validate(&suite).is_err());
}

#[test]
fn a_duplicate_program_is_rejected() {
    let mut suite = minimal();
    suite.programs.push(suite.programs[0].clone());
    assert!(validate(&suite).is_err());
}

#[test]
fn an_absolute_source_is_rejected() {
    let mut suite = minimal();
    suite.programs[0].sources = vec!["/etc/passwd".to_string()];
    assert!(validate(&suite).is_err());
}

#[test]
fn a_parent_directory_source_is_rejected() {
    let mut suite = minimal();
    suite.programs[0].sources = vec!["../escape.c".to_string()];
    assert!(validate(&suite).is_err());
}

#[test]
fn an_empty_program_name_is_rejected() {
    let mut suite = minimal();
    suite.programs[0].name.clear();
    assert!(validate(&suite).is_err());
}

#[test]
fn a_missing_suite_file_is_an_error() {
    assert!(load(std::path::Path::new("/nonexistent/suite.toml")).is_err());
}

#[test]
fn find_names_the_file() {
    let root = temp("find");
    let suites = root.join("suites");
    fs::create_dir_all(&suites).expect("suites");
    fs::write(
        suites.join("tiny.toml"),
        render_suite(
            &SuiteFile {
                suite: "tiny".to_string(),
                ..minimal()
            },
            "2026-11-14",
        )
        .expect("render"),
    )
    .expect("write");
    assert_eq!(find(&root, "tiny").expect("find").suite, "tiny");
    assert!(find(&root, "other").is_err());
}

#[test]
fn overrides_are_applied_by_suite_and_program() {
    let mut suite = minimal();
    let overrides = Overrides {
        entries: vec![
            Override {
                suite: "netbsd-libc".to_string(),
                program: "string/t_strchr".to_string(),
                cflags: vec!["-DFOO".to_string()],
                ldadd: vec!["-lm".to_string()],
                sources: Vec::new(),
                expect_exit: None,
                args: None,
            },
            Override {
                suite: "other".to_string(),
                program: "string/t_strchr".to_string(),
                cflags: vec!["-DBAR".to_string()],
                ldadd: Vec::new(),
                sources: Vec::new(),
                expect_exit: None,
                args: None,
            },
        ],
    };
    overrides.apply(&mut suite);
    assert_eq!(suite.programs[0].cflags, ["-DFOO"]);
    assert_eq!(suite.programs[0].ldadd, ["-lm"]);
}

#[test]
fn an_unparsed_program_maps_to_unparsed() {
    let mut suite = minimal();
    suite.programs[0].status = "unparsed".to_string();
    suite.programs[0].status_reason = Some("source not vendored".to_string());
    assert!(matches!(
        suite.programs[0].build_status(),
        BuildStatus::Unparsed(_)
    ));
}

#[test]
fn a_needs_program_maps_to_needs() {
    let mut suite = minimal();
    suite.programs[0].status = "needs-fs".to_string();
    assert_eq!(
        suite.programs[0].build_status(),
        BuildStatus::Needs("needs-fs".to_string())
    );
}

#[test]
fn generate_ltp_writes_an_external_suite() {
    let oracle = temp("ltp");
    let out = oracle.join("suites");
    let written = generate(&oracle, &out, Some("ltp"), "2026-11-14").expect("generate");
    assert_eq!(written, ["ltp"]);
    let suite = load(&out.join("ltp.toml")).expect("load");
    assert_eq!(suite.build.as_deref(), Some("external"));
    assert_eq!(suite.prebuilt.as_deref(), Some("<dir>"));
    assert!(suite.programs.is_empty());
}

#[test]
fn generate_all_skips_absent_vendors_but_never_invents_them() {
    let oracle = temp("empty");
    let out = oracle.join("suites");
    let written = generate(&oracle, &out, None, "2026-11-14").expect("generate");
    assert_eq!(written, ["ltp"]);
}

#[test]
fn generate_one_absent_suite_is_an_error() {
    let oracle = temp("absent");
    assert!(
        generate(
            &oracle,
            &oracle.join("suites"),
            Some("netbsd-libc"),
            "2026-11-14"
        )
        .is_err()
    );
}

#[test]
fn first_result_line_reads_a_result_file() {
    let root = temp("result");
    let path = root.join("result");
    fs::write(&path, "skipped: no network\n").expect("write");
    assert_eq!(first_result_line(&path), "no network");
    assert_eq!(first_result_line(&root.join("missing")), "");
}

#[test]
fn defaults_are_sane() {
    let suite = minimal();
    assert!(suite.srcdir_for_tests);
}
