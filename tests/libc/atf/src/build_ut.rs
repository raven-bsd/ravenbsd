// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::*;
use crate::suite::Program;
use crate::{BuildStatus, Framework, Mode};

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-build-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn program(name: &str, source: &str) -> Program {
    Program {
        name: name.to_string(),
        sources: vec![source.to_string()],
        cflags: Vec::new(),
        ldadd: Vec::new(),
        status: "ok".to_string(),
        status_reason: None,
        expect_exit: 0,
        args: Vec::new(),
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repository root")
        .to_path_buf()
}

fn builder(root: &Path) -> Builder {
    Builder {
        root: repo(),
        vendor: root.join("vendor"),
        out: root.join("out"),
        mode: Mode::Oracle,
        cc: "cc".to_string(),
        sysroot: None,
    }
}

#[test]
fn cc_is_needed_for_the_compile_tests() {
    if !cc_available() {
        eprintln!("SKIP: cc not found");
        assert!(
            std::env::var("RL_ATF_REQUIRED").ok().as_deref() != Some("1"),
            "cc is required but absent"
        );
    }
}

#[test]
fn classify_a_missing_header() {
    let text = "hello.c:1:10: fatal error: raven_missing.h: No such file or directory\n";
    assert_eq!(classify(text), "missing-header:raven_missing.h");
}

#[test]
fn classify_every_missing_symbol() {
    let text = "a.c:(.text+0x1): undefined reference to `foo'\nb.c:(.text+0x2): undefined reference to `bar'\n";
    let mut headers = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    classify_into(text, &mut headers, &mut symbols);
    assert_eq!(
        symbols.iter().cloned().collect::<Vec<_>>(),
        ["bar".to_string(), "foo".to_string()]
    );
}

#[test]
fn classify_an_implicit_declaration() {
    let text = "a.c:2:12: warning: implicit declaration of function ‘baz’\n";
    assert_eq!(classify(text), "missing-symbol:baz");
}

#[test]
fn classify_other_errors() {
    let text = "a.c:1:1: error: expected ';'\n";
    assert!(classify(text).starts_with("build-other:"));
}

#[test]
fn classify_no_output() {
    assert_eq!(classify(""), "build-other:compiler failed with no output");
}

#[test]
fn sysroot_missing_names_the_first_absent_path() {
    let root = temp("sysroot");
    assert_eq!(
        sysroot_missing(&root),
        Some(root.join("usr/include/stdio.h"))
    );
}

#[test]
fn a_plain_program_builds() {
    if !cc_available() {
        eprintln!("SKIP: cc not found");
        return;
    }
    let root = temp("plain");
    let suite = root.join("vendor/suite");
    fs::create_dir_all(&suite).expect("suite");
    fs::write(suite.join("hello.c"), "int main(void) { return 0; }\n").expect("source");
    let report = builder(&root)
        .build(
            "test-suite",
            "suite",
            Framework::ExitStatus,
            &program("hello", "hello.c"),
        )
        .expect("build");
    assert_eq!(report.status, BuildStatus::Built);
    assert!(report.binary.expect("binary").is_file());
}

#[test]
fn a_missing_header_is_a_classified_failure() {
    if !cc_available() {
        eprintln!("SKIP: cc not found");
        return;
    }
    let root = temp("missing-header");
    let suite = root.join("vendor/suite");
    fs::create_dir_all(&suite).expect("suite");
    fs::write(
        suite.join("hello.c"),
        "#include <raven_missing.h>\nint main(void) { return 0; }\n",
    )
    .expect("source");
    let report = builder(&root)
        .build(
            "test-suite",
            "suite",
            Framework::ExitStatus,
            &program("hello", "hello.c"),
        )
        .expect("build");
    assert_eq!(
        report.status,
        BuildStatus::Failed("missing-header:raven_missing.h".to_string())
    );
    assert!(report.missing_headers.contains("raven_missing.h"));
}

#[test]
fn an_undefined_function_is_a_missing_symbol() {
    if !cc_available() {
        eprintln!("SKIP: cc not found");
        return;
    }
    let root = temp("missing-symbol");
    let suite = root.join("vendor/suite");
    fs::create_dir_all(&suite).expect("suite");
    fs::write(
        suite.join("hello.c"),
        "int missing_one(void); int missing_two(void);\nint main(void) { return missing_one() + missing_two(); }\n",
    )
    .expect("source");
    let report = builder(&root)
        .build(
            "test-suite",
            "suite",
            Framework::ExitStatus,
            &program("hello", "hello.c"),
        )
        .expect("build");
    assert!(report.status.reason().starts_with("missing-symbol:"));
    assert_eq!(report.missing_symbols.len(), 2);
}

#[test]
fn atf_sources_skip_tests_and_helpers() {
    let root = temp("atf-sources");
    for directory in ATF_SOURCES {
        fs::create_dir_all(root.join(directory)).expect("dir");
    }
    fs::write(root.join("atf-c/keep.c"), "").expect("write");
    fs::write(root.join("atf-c/skip_test.c"), "").expect("write");
    fs::write(root.join("atf-c/h_skip.c"), "").expect("write");
    fs::write(root.join("atf-c/detail/helper.c"), "").expect("write");
    fs::write(root.join("atf-c/detail/atf_c_test.c"), "").expect("write");
    let sources = atf_sources(&root).expect("sources");
    assert_eq!(sources, [PathBuf::from("atf-c/keep.c")]);
}

#[test]
fn atf_string_definitions_are_complete() {
    let definitions = atf_string_definitions(Mode::Oracle).join(" ");
    assert!(definitions.contains("ATF_SHELL=\"/bin/sh\""));
    assert!(definitions.contains("ATF_WORKDIR=\"/tmp\""));
    assert!(definitions.contains("ATF_ARCH=\"x86_64\""));
}

#[test]
fn a_target_build_without_a_sysroot_is_an_error() {
    let root = temp("no-sysroot");
    let mut value = builder(&root);
    value.mode = Mode::Target;
    let suite = root.join("vendor/suite");
    fs::create_dir_all(&suite).expect("suite");
    fs::write(suite.join("hello.c"), "int main(void) { return 0; }\n").expect("source");
    let error = value
        .build(
            "test-suite",
            "suite",
            Framework::ExitStatus,
            &program("hello", "hello.c"),
        )
        .expect_err("must fail");
    assert!(error.to_string().contains("--sysroot"));
}
