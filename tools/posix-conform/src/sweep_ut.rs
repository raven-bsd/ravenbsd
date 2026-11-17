// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the interface sweep classification.

use std::collections::BTreeSet;

use crate::sweep::{Class, classify, compiler, load_extensions};

fn extensions(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn extension_list_ignores_comments_and_blanks() {
    let path = std::env::temp_dir().join(format!(
        "posix-sweep-ext-{}-{}",
        std::process::id(),
        "parse"
    ));
    std::fs::write(&path, "# SPDX\nbrk\n\n foo  # trailing comment\n# bar\n").unwrap();
    let names = load_extensions(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(names, extensions(&["brk", "foo"]));
}

#[test]
fn a_passing_row_is_pass() {
    assert_eq!(
        classify("read", false, &extensions(&[]), false),
        Class::Pass
    );
    assert_eq!(
        classify("brk", false, &extensions(&["brk"]), true),
        Class::Pass
    );
}

#[test]
fn a_listed_failure_is_an_extension() {
    let names = extensions(&["brk"]);
    assert_eq!(classify("brk", true, &names, false), Class::Extension);
    assert_eq!(classify("eaccess", true, &names, false), Class::Required);
}

#[test]
fn strict_mode_turns_extensions_into_failures() {
    let names = extensions(&["brk"]);
    assert_eq!(classify("brk", true, &names, true), Class::Required);
}

#[test]
fn unknown_arch_is_rejected() {
    assert!(compiler("riscv64").is_err());
}
