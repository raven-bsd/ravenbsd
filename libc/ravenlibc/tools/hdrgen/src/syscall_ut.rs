// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the syscall header generator.

use super::*;

#[test]
fn every_architecture_has_a_table() {
    for arch in ARCHES {
        assert!(table(arch).is_some(), "{arch}");
        assert_eq!(table_len(arch).unwrap(), table(arch).unwrap().len());
    }
    assert!(table("mips").is_none());
}

#[test]
fn read_numbers_match_the_linux_tables() {
    assert_eq!(number("x86_64", "read"), Some(0));
    assert_eq!(number("aarch64", "read"), Some(63));
    assert_eq!(number("riscv64", "read"), Some(63));
}

#[test]
fn underscore_names_keep_their_underscore() {
    assert!(number("x86_64", "_sysctl").is_some());
    let text = render("x86_64", "rev").expect("render");
    assert!(text.contains("#define __NR__sysctl "));
    assert!(text.contains("#define SYS__sysctl __NR__sysctl"));
}

#[test]
fn header_has_two_defines_per_name() {
    for arch in ARCHES {
        let text = render(arch, "rev").expect("render");
        let lines = text
            .lines()
            .filter(|line| line.starts_with("#define "))
            .count();
        assert_eq!(lines, table_len(arch).unwrap() * 2, "{arch}");
    }
}

#[test]
fn names_are_sorted() {
    let text = render("x86_64", "rev").expect("render");
    let mut names: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("#define __NR_"))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();
    let sorted = {
        let mut copy = names.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(names, sorted);
    names.dedup();
    assert_eq!(names.len(), table_len("x86_64").unwrap());
}

#[test]
fn header_carries_the_linux_raw_sys_notice() {
    let text = render("aarch64", "abc123").expect("render");
    assert!(text.contains("SPDX-License-Identifier: MIT"));
    assert!(text.contains("linux-raw-sys abc123 src/aarch64/general.rs"));
    assert!(text.contains("Do not edit"));
}

#[test]
fn render_all_writes_three_headers() {
    let files = render_all("rev").expect("render all");
    assert_eq!(files.len(), 3);
    for arch in ARCHES {
        assert!(files.contains_key(&format!("arch/{arch}/bits/syscall.h")));
    }
}

#[test]
fn tables_have_no_duplicate_names() {
    for arch in ARCHES {
        let mut names: Vec<&str> = table(arch).unwrap().iter().map(|(name, _)| *name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "{arch}");
    }
}
