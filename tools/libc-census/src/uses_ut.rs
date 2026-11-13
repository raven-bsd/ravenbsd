// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for S1/S3 source scans and S2 undefined lists.

use super::*;
use crate::model::Kind;
use crate::testutil::{TempDir, fixture};

fn universe(names: &[&str]) -> Universe {
    Universe::build(names.iter().map(|name| ((*name).to_string(), Kind::Func)))
}

#[test]
fn angle_includes_count_once_per_package() {
    let output = scan_sources(
        &fixture("sources"),
        &universe(&["strlcpy", "epoll_create", "memchr"]),
    )
    .unwrap();
    assert_eq!(output.headers["sys/queue.h"], 2);
    assert_eq!(output.headers["stdio.h"], 1);
    assert_eq!(output.packages, 2);
}

#[test]
fn call_sites_count_once_per_package() {
    let output = scan_sources(
        &fixture("sources"),
        &universe(&["strlcpy", "epoll_create", "memchr"]),
    )
    .unwrap();
    assert_eq!(output.symbols["strlcpy"], 1);
    assert_eq!(output.symbols["memchr"], 1);
    assert_eq!(output.symbols["epoll_create"], 1);
}

#[test]
fn dash_l_flags_are_extracted() {
    let output = scan_sources(
        &fixture("sources"),
        &universe(&["strlcpy", "epoll_create", "memchr"]),
    )
    .unwrap();
    assert_eq!(output.libraries["m"].packages, 1);
    assert_eq!(output.libraries["pthread"].packages, 1);
    assert_eq!(output.libraries["foo"].packages, 1);
}

#[test]
fn ac_check_lib_is_extracted() {
    let output = scan_sources(&fixture("sources"), &universe(&["strlcpy"])).unwrap();
    assert_eq!(output.libraries["rt"].packages, 1);
}

#[test]
fn ac_search_libs_lists_every_library() {
    let output = scan_sources(&fixture("sources"), &universe(&["strlcpy"])).unwrap();
    assert!(output.libraries.contains_key("m"));
    assert!(output.libraries.contains_key("pthread"));
}

#[test]
fn stub_libraries_are_flagged() {
    let output = scan_sources(&fixture("sources"), &universe(&["strlcpy"])).unwrap();
    assert!(output.libraries["m"].stub);
    assert!(output.libraries["pthread"].stub);
    assert!(output.libraries["rt"].stub);
    assert!(!output.libraries["foo"].stub);
}

#[test]
fn a_missing_sources_tree_is_skipped() {
    let output = scan_sources(&fixture("no-such-sources"), &universe(&["strlcpy"])).unwrap();
    assert!(output.skipped.as_deref().unwrap().starts_with("skipped:"));
}

#[test]
fn only_angle_includes_are_collected() {
    let temp = TempDir::new("uses-include");
    temp.write("one/x.c", "#include <sys/queue.h>\n#include \"local.h\"\n");
    let output = scan_sources(temp.path(), &universe(&["strlcpy"])).unwrap();
    assert!(output.headers.contains_key("sys/queue.h"));
    assert!(!output.headers.contains_key("local.h"));
}

#[test]
fn find_library_is_not_parsed() {
    let temp = TempDir::new("uses-find");
    temp.write("one/meson.build", "dep = find_library('foo')\n");
    let output = scan_sources(temp.path(), &universe(&["strlcpy"])).unwrap();
    assert!(output.libraries.is_empty());
}

#[test]
fn undef_lists_split_strong_and_weak() {
    let output = read_undef(&fixture("undef")).unwrap();
    assert_eq!(output.strong["strlcpy"], 1);
    assert_eq!(output.weak["strlcpy"], 1);
    assert_eq!(output.weak["arc4random"], 1);
    assert_eq!(output.strong["epoll_create"], 1);
    assert!(output.skipped.is_none());
}

#[test]
fn a_missing_undef_directory_is_skipped() {
    let output = read_undef(&fixture("no-such-undef")).unwrap();
    assert!(output.skipped.as_deref().unwrap().starts_with("skipped:"));
}

#[test]
fn malformed_undef_lines_are_ignored() {
    let temp = TempDir::new("undef-malformed");
    temp.write("pkg.undef", "good\tU\nno-tab\nbad\tX\nother\tw\n");
    let output = read_undef(temp.path()).unwrap();
    assert_eq!(output.strong["good"], 1);
    assert_eq!(output.weak["other"], 1);
    assert_eq!(output.strong.len(), 1);
}
