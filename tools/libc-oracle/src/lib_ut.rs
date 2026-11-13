// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::{Path, PathBuf};

use super::*;
use crate::testutil::TempDir;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_recipes() -> String {
    fixtures().join("recipes").display().to_string()
}

#[test]
fn help_is_success_and_no_arguments_is_an_error() {
    assert_eq!(run(&["--help".to_string()]).unwrap(), 0);
    assert!(run(&[]).is_err());
}

#[test]
fn an_unknown_subcommand_is_an_error() {
    let error = run(&["wat".to_string()]).unwrap_err();
    assert!(error.to_string().contains("unknown subcommand"));
}

#[test]
fn list_prints_fixture_families() {
    let code = command_list(&["--recipes".to_string(), fixture_recipes()]).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn build_needs_a_family_or_all() {
    let error = command_build(&[
        "--recipes".to_string(),
        fixture_recipes(),
        "--resources".to_string(),
        fixtures().display().to_string(),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("family or --all"));
}

#[test]
fn build_refuses_a_family_and_all_together() {
    let error = command_build(&[
        "fixture".to_string(),
        "--all".to_string(),
        "--recipes".to_string(),
        fixture_recipes(),
        "--resources".to_string(),
        fixtures().display().to_string(),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("not both"));
}

#[test]
fn adhoc_needs_two_positionals() {
    let error = command_adhoc(&["openbsd".to_string()]).unwrap_err();
    assert!(error.to_string().contains("adhoc needs"));
}

#[test]
fn adhoc_paths_are_restricted() {
    assert!(valid_adhoc_path("lib/libc/string/strlcpy.c", "c"));
    assert!(!valid_adhoc_path("/lib/libc/string/strlcpy.c", "c"));
    assert!(!valid_adhoc_path("lib/../strlcpy.c", "c"));
    assert!(!valid_adhoc_path("lib/libc/string/strlcpy.S", "c"));
    assert!(valid_adhoc_path("include/chacha_private.h", "header"));
    assert!(!valid_adhoc_path("include/chacha_private.c", "header"));
}

#[test]
fn check_reports_skipped_for_an_empty_resource_tree() {
    let temp = TempDir::new("check-empty");
    let code = command_check(&[
        "--recipes".to_string(),
        fixture_recipes(),
        "--resources".to_string(),
        temp.path().display().to_string(),
    ])
    .expect("check");
    assert_eq!(code, 0);
}

#[test]
fn vendor_requires_the_three_paths() {
    let error = command_vendor(&["--from".to_string(), "x".to_string()]).unwrap_err();
    assert!(error.to_string().contains("--donor"));
    let error = command_vendor(&["--donor".to_string(), "fixture".to_string()]).unwrap_err();
    assert!(error.to_string().contains("--from"));
    let error = command_vendor(&[
        "--donor".to_string(),
        "fixture".to_string(),
        "--from".to_string(),
        "x".to_string(),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("--to"));
}
