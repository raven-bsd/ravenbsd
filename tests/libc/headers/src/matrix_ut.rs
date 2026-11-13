// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for matrix helpers.

use super::*;

#[test]
fn t1_tu_count_follows_the_formula() {
    let headers = vec!["stdio.h".to_string(), "time.h".to_string()];
    assert_eq!(t1_tus(&headers, false).len(), 2 * 3 * 6 * 5);
    assert_eq!(t1_tus(&headers, true).len(), 2 * 2 * 2);
}

#[test]
fn standalone_source_is_not_empty() {
    let source = standalone_source("stdio.h");
    assert!(source.starts_with("#include <stdio.h>"));
    assert!(source.contains("int main"));
}

#[test]
fn every_mode_name_parses() {
    for name in FEATURE_MODES {
        assert!(mode(name).is_some(), "{name}");
    }
    assert!(mode("bogus").is_none());
}

#[test]
fn visibility_values_need_all_five_macros() {
    let dump = vec![
        ("__POSIX_VISIBLE".to_string(), "202405".to_string()),
        ("__XSI_VISIBLE".to_string(), "800".to_string()),
        ("__BSD_VISIBLE".to_string(), "1".to_string()),
        ("__ISO_C_VISIBLE".to_string(), "2023".to_string()),
    ];
    assert!(visibility_values(&dump).is_none());
}

#[test]
fn probe_arguments_include_the_header_and_target() {
    let toolchain = crate::clang::locate_toolchain().expect("clang");
    let args = probe_args(
        &toolchain,
        Arch::X86_64,
        Lang::Gnu17,
        Vis::Default,
        "stdio.h",
        "printf",
    );
    assert!(args[0].starts_with("--target=x86_64-unknown-"));
    assert_eq!(args.last().map(String::as_str), Some("-"));
}
