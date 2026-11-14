// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::PathBuf;

use super::*;

fn failing(text: &str) -> Vec<StepLog> {
    vec![StepLog {
        name: "make".to_string(),
        exit: Some(2),
        text: text.to_string(),
    }]
}

fn successful(text: &str) -> Vec<StepLog> {
    vec![StepLog {
        name: "configure".to_string(),
        exit: Some(0),
        text: text.to_string(),
    }]
}

fn result(text: &str) -> Classified {
    classify(&failing(text), &SymbolIndex::default(), &[]).expect("classified")
}

#[test]
fn clang_missing_header() {
    let value = result("a.c:1:10: fatal error: 'sys/epoll.h' file not found\n");
    assert_eq!(value.class, "missing-header");
    assert_eq!(value.detail, "sys/epoll.h");
    assert_eq!(value.findings.get("missing-header"), Some(&1));
}

#[test]
fn gcc_missing_header() {
    let value = result("a.c:1:10: fatal error: sys/epoll.h: No such file or directory\n");
    assert_eq!(value.class, "missing-header");
    assert_eq!(value.detail, "sys/epoll.h");
}

#[test]
fn two_missing_headers_are_kept() {
    let value = result("fatal error: 'a.h' file not found\nfatal error: 'b.h' file not found\n");
    assert_eq!(value.missing_headers.len(), 2);
    assert_eq!(value.findings.get("missing-header"), Some(&2));
}

#[test]
fn clang_implicit_declaration_is_a_missing_symbol() {
    let value = result("a.c:1:9: error: call to undeclared function 'strlcpy'\n");
    assert_eq!(value.class, "missing-symbol");
    assert_eq!(value.detail, "strlcpy");
    assert!(value.implicit_decls.contains("strlcpy"));
}

#[test]
fn gcc_implicit_declaration_is_a_missing_symbol() {
    let value = result("a.c:1:9: warning: implicit declaration of function 'strlcpy'\n");
    assert_eq!(value.class, "missing-symbol");
    assert!(value.implicit_decls.contains("strlcpy"));
}

#[test]
fn lld_undefined_symbol() {
    let value = result("ld.lld: error: undefined symbol: getauxval\n");
    assert_eq!(value.class, "missing-symbol");
    assert_eq!(value.detail, "getauxval");
}

#[test]
fn gnu_undefined_reference_quoted() {
    let value = result("/usr/bin/ld: a.o: undefined reference to 'strlcpy'\n");
    assert_eq!(value.class, "missing-symbol");
    assert_eq!(value.detail, "strlcpy");
}

#[test]
fn gnu_undefined_reference_backtick() {
    let value = result("/usr/bin/ld: a.o: undefined reference to `strlcpy'\n");
    assert_eq!(value.class, "missing-symbol");
    assert_eq!(value.detail, "strlcpy");
}

#[test]
fn two_missing_symbols_are_kept() {
    let value =
        result("ld.lld: error: undefined symbol: foo\nld.lld: error: undefined symbol: bar\n");
    assert_eq!(value.missing_symbols.len(), 2);
    assert_eq!(value.findings.get("missing-symbol"), Some(&2));
}

#[test]
fn lld_missing_library() {
    let value = result("ld.lld: error: unable to find library -lfoo\n");
    assert_eq!(value.class, "missing-lib");
    assert_eq!(value.detail, "foo");
}

#[test]
fn gnu_missing_library() {
    let value = result("/usr/bin/ld: cannot find -lbar\n");
    assert_eq!(value.class, "missing-lib");
    assert_eq!(value.detail, "bar");
}

#[test]
fn configure_error_line() {
    let value = result("configure: error: C compiler cannot create executables\n");
    assert_eq!(value.class, "configure");
    assert!(value.detail.contains("C compiler"));
}

#[test]
fn a_failed_configure_step_is_configure_class() {
    let logs = vec![StepLog {
        name: "configure".to_string(),
        exit: Some(1),
        text: "checking for cc... ok\n".to_string(),
    }];
    let value = classify(&logs, &SymbolIndex::default(), &[]).expect("classified");
    assert_eq!(value.class, "configure");
}

#[test]
fn invalid_configuration_is_platform() {
    let value = result("config.sub: Invalid configuration `x86_64-unknown-ravenbsd'\n");
    assert_eq!(value.class, "platform");
    assert!(value.detail.contains("config.sub"));
}

#[test]
fn unsupported_system_is_platform() {
    let value = result("error: unsupported system `ravenbsd`\n");
    assert_eq!(value.class, "platform");
}

#[test]
fn cmake_missing_module_is_platform() {
    let value = result("CMake Error: cmake: could not find thread library\n");
    assert_eq!(value.class, "platform");
}

#[test]
fn a_plain_error_is_other() {
    let value = result("make: *** [all] Error 1\n");
    assert_eq!(value.class, "other");
    assert!(value.detail.contains("Error"));
}

#[test]
fn missing_header_wins_over_missing_symbol() {
    let value =
        result("ld.lld: error: undefined symbol: strlcpy\nfatal error: 'foo.h' file not found\n");
    assert_eq!(value.class, "missing-header");
    assert!(value.findings.contains_key("missing-symbol"));
    assert!(value.findings.contains_key("missing-header"));
}

#[test]
fn a_header_probe_in_the_index_is_a_missing_header() {
    let index = SymbolIndex::from_names(&[], &["sys/epoll.h"]);
    let value =
        classify(&failing("checking for sys/epoll.h... no\n"), &index, &[]).expect("classified");
    assert_eq!(value.class, "missing-header");
    assert_eq!(value.detail, "sys/epoll.h");
}

#[test]
fn a_symbol_probe_in_the_index_is_a_missing_symbol_and_a_probe_failure() {
    let index = SymbolIndex::from_names(&["strlcpy"], &["string.h"]);
    let value =
        classify(&failing("checking for strlcpy... no\n"), &index, &[]).expect("classified");
    assert_eq!(value.class, "missing-symbol");
    assert!(value.libc_probe_failed.contains("strlcpy"));
}

#[test]
fn an_unknown_probe_is_not_a_finding() {
    let index = SymbolIndex::from_names(&["strlcpy"], &["string.h"]);
    let value = classify(&successful("checking for wizbang... no\n"), &index, &[]);
    assert!(value.is_none());
}

#[test]
fn an_incomplete_index_skips_probe_rules() {
    let index = SymbolIndex::default();
    let value = classify(&successful("checking for strlcpy... no\n"), &index, &[]);
    assert!(value.is_none());
}

#[test]
fn a_yes_probe_is_ignored() {
    let index = SymbolIndex::from_names(&["strlcpy"], &["string.h"]);
    let value = classify(&successful("checking for strlcpy... yes\n"), &index, &[]);
    assert!(value.is_none());
}

#[test]
fn a_long_error_is_truncated_to_two_hundred_bytes() {
    let text = format!("error: {}\n", "x".repeat(500));
    let value = result(&text);
    assert!(value.first_error.expect("first").len() <= 203);
}

#[test]
fn first_error_replaces_absolute_paths() {
    let log = StepLog {
        name: "make".to_string(),
        exit: Some(2),
        text: "error: cannot open /w/build/pkg/src/a.c\n".to_string(),
    };
    let replacements = vec![(PathBuf::from("/w/build/pkg/src"), "<src>")];
    let value = classify(&[log], &SymbolIndex::default(), &replacements).expect("classified");
    let first = value.first_error.expect("first");
    assert!(first.contains("<src>/a.c"));
    assert!(!first.contains("/w/build"));
}

#[test]
fn successful_steps_do_not_classify() {
    let logs = vec![StepLog {
        name: "make".to_string(),
        exit: Some(0),
        text: "all good\n".to_string(),
    }];
    assert!(classify(&logs, &SymbolIndex::default(), &[]).is_none());
}

#[test]
fn a_failing_step_without_a_message_is_other() {
    let logs = vec![StepLog {
        name: "make".to_string(),
        exit: Some(2),
        text: "silent\n".to_string(),
    }];
    let value = classify(&logs, &SymbolIndex::default(), &[]).expect("classified");
    assert_eq!(value.class, "other");
    assert_eq!(value.detail, "");
}

#[test]
fn implicit_and_link_errors_for_the_same_symbol_are_deduplicated() {
    let value = result(
        "error: call to undeclared function 'strlcpy'\nld.lld: error: undefined symbol: strlcpy\n",
    );
    assert_eq!(value.missing_symbols.len(), 1);
}

#[test]
fn details_are_deterministic() {
    let first =
        result("ld.lld: error: undefined symbol: zebra\nld.lld: error: undefined symbol: alpha\n");
    let second =
        result("ld.lld: error: undefined symbol: alpha\nld.lld: error: undefined symbol: zebra\n");
    assert_eq!(first.detail, second.detail);
    assert_eq!(first.detail, "alpha");
}

#[test]
fn missing_lib_details_are_deterministic() {
    let value = result(
        "ld.lld: error: unable to find library -lz\nld.lld: error: unable to find library -la\n",
    );
    assert_eq!(value.detail, "a");
}

#[test]
fn a_symbol_index_loads_ring_one_and_two_only() {
    let temp = crate::testutil::TempDir::new("classify-index");
    let path = temp.join("symbols.toml");
    crate::testutil::write(
        &path,
        "schema = 1\n[[symbol]]\nname = \"strlcpy\"\nring = 1\nheader = \"string.h\"\n[[symbol]]\nname = \"getauxval\"\nring = 2\nheader = \"sys/auxv.h\"\n[[symbol]]\nname = \"rare\"\nring = 3\nheader = \"rare.h\"\n",
    );
    let index = SymbolIndex::load(&path).expect("load");
    assert!(index.complete);
    assert!(index.symbols.contains("strlcpy"));
    assert!(index.symbols.contains("getauxval"));
    assert!(!index.symbols.contains("rare"));
    assert!(index.headers.contains("string.h"));
    assert!(!index.headers.contains("rare.h"));
}

#[test]
fn a_missing_symbol_index_is_incomplete() {
    let temp = crate::testutil::TempDir::new("classify-missing-index");
    let index = SymbolIndex::load(&temp.join("nope.toml")).expect("load");
    assert!(!index.complete);
    assert!(index.symbols.is_empty());
}

#[test]
fn a_platform_detail_does_not_override_a_symbol() {
    let value = result(
        "config.sub: Invalid configuration `x86_64-unknown-ravenbsd'\nld.lld: error: undefined symbol: foo\n",
    );
    assert_eq!(value.class, "missing-symbol");
    assert!(value.findings.contains_key("platform"));
}
