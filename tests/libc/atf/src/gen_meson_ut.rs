// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::Framework;

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "raven-atf-gen-meson-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(&path, text).expect("write");
    }
    root
}

#[test]
fn parses_the_first_test_case_list() {
    let text = "all_test_cases = [\n  'ansi/alloc',\n  'posix/access',\n]\nall_test_cases += ['later/x']\n";
    assert_eq!(parse_all_test_cases(text), ["ansi/alloc", "posix/access"]);
}

#[test]
fn parses_a_single_line_list() {
    assert_eq!(
        parse_all_test_cases("all_test_cases = [ 'ansi/one', 'posix/two' ]\n"),
        ["ansi/one", "posix/two"]
    );
}

#[test]
fn stops_at_the_closing_bracket() {
    let text = "all_test_cases = [\n 'ansi/one',\n]\n# 'ignored/no'\n";
    assert_eq!(parse_all_test_cases(text), ["ansi/one"]);
}

#[test]
fn an_empty_list_is_empty() {
    assert!(parse_all_test_cases("all_test_cases = [\n]\n").is_empty());
    assert!(parse_all_test_cases("nothing here\n").is_empty());
}

#[test]
fn scan_mlibc_keeps_c_tests_and_marks_rtld_dynamic() {
    let root = tree(
        "mlibc",
        &[
            (
                "tests/meson.build",
                "all_test_cases = [\n 'ansi/alloc',\n 'posix/access',\n 'rtld/dlopen',\n 'bsd/strl',\n]\n",
            ),
            ("tests/ansi/alloc.c", ""),
            ("tests/posix/access.c", ""),
            ("tests/rtld/dlopen.c", ""),
            ("tests/bsd/strl.c", ""),
        ],
    );
    let programs = scan_mlibc(&root).expect("scan");
    let names: Vec<&str> = programs
        .iter()
        .map(|program| program.name.as_str())
        .collect();
    assert_eq!(names, ["ansi/alloc", "posix/access", "rtld/dlopen"]);
    assert_eq!(programs[2].status, "needs-dynamic");
    assert_eq!(programs[0].framework, Framework::ExitStatus);
}

#[test]
fn scan_mlibc_marks_a_cpp_test_unparsed() {
    let root = tree(
        "mlibc-cpp",
        &[
            (
                "tests/meson.build",
                "all_test_cases = [\n 'ansi/thing',\n]\n",
            ),
            ("tests/ansi/thing.c", ""),
        ],
    );
    let programs = scan_mlibc(&root).expect("scan");
    assert_eq!(
        programs[0].status,
        "needs-dynamic".to_string().replace("needs-dynamic", "ok")
    );
}

#[test]
fn scan_mlibc_marks_missing_source_unparsed() {
    let root = tree(
        "mlibc-missing",
        &[(
            "tests/meson.build",
            "all_test_cases = [\n 'ansi/gone',\n]\n",
        )],
    );
    let programs = scan_mlibc(&root).expect("scan");
    assert_eq!(programs[0].status, "unparsed");
    assert!(
        programs[0]
            .status_reason
            .as_deref()
            .unwrap_or("")
            .contains("not vendored")
    );
}

#[test]
fn scan_mlibc_requires_the_meson_file() {
    let root = tree("mlibc-empty", &[("README", "")]);
    assert!(scan_mlibc(&root).is_err());
}

#[test]
fn scan_picolibc_finds_top_level_and_libc_testsuite() {
    let root = tree(
        "pico",
        &[
            ("test-stdio.c", ""),
            ("test-math.c", ""),
            ("helper.c", ""),
            ("libc-testsuite/qsort.c", ""),
            ("libc-testsuite/readme.txt", ""),
        ],
    );
    let programs = scan_picolibc(&root).expect("scan");
    let names: Vec<&str> = programs
        .iter()
        .map(|program| program.name.as_str())
        .collect();
    assert_eq!(names, ["libc-testsuite/qsort", "test-math", "test-stdio"]);
    assert_eq!(programs[0].sources, ["libc-testsuite/qsort.c"]);
}

#[test]
fn scan_picolibc_ignores_a_missing_tree() {
    let programs = scan_picolibc(Path::new("/nonexistent/picolibc")).expect_err("must fail");
    let _ = programs;
}

#[test]
fn scan_pjdfstest_marks_needs_fs() {
    let root = tree(
        "pjd",
        &[
            ("chmod/01.t", "#!/bin/sh\n"),
            ("open/02.t", "#!/bin/sh\n"),
            ("open/README", ""),
        ],
    );
    let programs = scan_pjdfstest(&root).expect("scan");
    let names: Vec<&str> = programs
        .iter()
        .map(|program| program.name.as_str())
        .collect();
    assert_eq!(names, ["chmod/01", "open/02"]);
    assert_eq!(programs[0].framework, Framework::Tap);
    assert_eq!(programs[0].status, "needs-fs");
}

#[test]
fn scan_pjdfstest_ignores_a_missing_tree() {
    assert!(
        scan_pjdfstest(Path::new("/nonexistent/pjd"))
            .expect("scan")
            .is_empty()
    );
}

#[test]
fn status_of_maps_ok_and_needs() {
    let program = MakeProgram {
        name: "x".to_string(),
        sources: vec!["x.c".to_string()],
        cflags: Vec::new(),
        ldadd: Vec::new(),
        framework: Framework::ExitStatus,
        status: "ok".to_string(),
        status_reason: None,
    };
    assert!(matches!(status_of(&program), crate::BuildStatus::Built));
}
