// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the layout database.

use super::*;

#[test]
fn parses_rows_and_fields() {
    let rows = parse(
        r#"
[[type]]
name = "struct stat"
headers = ["sys/types.h"]
size = 128
align = 8
arches = ["x86_64"]
[[type.field]]
name = "st_dev"
offset = 0
"#,
    )
    .expect("parse");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].size, 128);
    assert_eq!(rows[0].fields[0].name, "st_dev");
}

#[test]
fn real_database_has_at_least_75_rows() {
    let rows = load().expect("layouts");
    assert!(rows.len() >= 75, "only {} rows", rows.len());
    assert!(rows.iter().any(|row| row.name == "struct stat"));
    assert!(rows.iter().any(|row| row.name == "jmp_buf"));
    assert!(
        rows.iter()
            .any(|row| row.name == "nlink_t" && row.size == 4)
    );
}

#[test]
fn availability_follows_the_manifest_not_a_hardcoded_owner() {
    let rows = load().expect("layouts");
    let headers = crate::manifest::load().expect("manifest");
    let statvfs = rows
        .iter()
        .find(|row| row.name == "struct statvfs")
        .expect("statvfs row");
    assert!(available(statvfs, Arch::X86_64, &headers));
    let regex = rows
        .iter()
        .find(|row| row.name == "regex_t")
        .expect("regex_t row");
    assert!(available(regex, Arch::X86_64, &headers));
    let mut missing = statvfs.clone();
    missing.headers = vec!["definitely-not-installed.h".to_string()];
    assert!(!available(&missing, Arch::X86_64, &headers));
    assert_eq!(owner(&missing, &headers), "C37");
}

#[test]
fn assert_source_uses_expected_macros() {
    let rows = load().expect("layouts");
    let headers = crate::manifest::load().expect("manifest");
    let source = assert_source(&rows, Arch::X86_64, &headers);
    assert!(source.contains("_Static_assert(sizeof(struct stat) == 128"));
    assert!(source.contains("_Alignof(jmp_buf) == 8"));
    assert!(source.contains("offsetof(struct dirent, d_name) == 19"));
    assert!(source.contains("sizeof(struct statvfs)"));
    assert!(source.contains("sizeof(regex_t) == 64"));
}
