// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the generic-instantiation counter.

use super::*;

const NM: &str = "\
0000000000000010 0000000000000010 T raven_libc::to_u64
0000000000000010 0000000000000010 T raven_libc::to_u32
0000000000000010 0000000000000010 T raven_libc::to_u16
0000000000000010 0000000000000010 T <raven_libc::Vec<u8>>::push
0000000000000010 0000000000000010 T <raven_libc::Vec<u16>>::push
0000000000000010 0000000000000010 T <raven_libc::Vec<u32>>::push
0000000000000010 0000000000000010 T <raven_libc::Vec<u64>>::push
0000000000000010 0000000000000010 T <raven_libc::Vec<i8>>::len
";

#[test]
fn groups_count_by_the_prefix_before_the_angle() {
    let groups = groups(NM);
    let vec_group = groups
        .iter()
        .find(|group| group.prefix == "raven_libc::Vec")
        .expect("Vec group");
    assert_eq!(vec_group.count, 5);
    assert_eq!(
        vec_group.examples.first().map(String::as_str),
        Some("<raven_libc::Vec<u8>>::push")
    );
}

#[test]
fn non_generic_names_are_counted_once() {
    let groups = groups(NM);
    assert_eq!(groups.len(), 4);
    assert!(
        groups
            .iter()
            .any(|group| group.prefix == "raven_libc::to_u64" && group.count == 1)
    );
}

#[test]
fn groups_sort_by_count_then_prefix() {
    let groups = groups(NM);
    assert_eq!(groups.first().expect("first").prefix, "raven_libc::Vec");
}

#[test]
fn four_instantiations_fail_without_an_exception() {
    let groups = groups(NM);
    let allow = AllowFile::default();
    let failures = failures(&groups, &allow);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures.first().expect("one").prefix, "raven_libc::Vec");
}

#[test]
fn two_instantiations_pass() {
    let text = "\
0000 0010 T <x::A<u8>>::f
0000 0010 T <x::A<u16>>::f
";
    assert!(failures(&groups(text), &AllowFile::default()).is_empty());
}

#[test]
fn an_allowlisted_prefix_passes() {
    let allow = AllowFile {
        monomorph: vec![MonomorphAllow {
            prefix: "raven_libc::Vec".to_string(),
            reason: "documented test".to_string(),
        }],
    };
    assert!(failures(&groups(NM), &allow).is_empty());
}

#[test]
fn top_limits_the_report() {
    let groups = groups(NM);
    assert_eq!(top(&groups, 2).len(), 2);
}

#[test]
fn render_marks_a_failure() {
    let text = render(&groups(NM), &AllowFile::default());
    assert!(text.contains("FAIL"));
    assert!(text.contains("above 2 and not allow-listed"));
}

#[test]
fn allowlist_parses_and_a_missing_file_is_empty() {
    let dir = crate::testutil::TempDir::new("monomorph-allow").expect("temp");
    assert!(
        load_allowlist(&dir.path().join("missing.toml"))
            .expect("missing")
            .monomorph
            .is_empty()
    );
    let path = dir
        .write(
            "size-allowlist.toml",
            b"[[monomorph]]\nprefix = \"raven_libc::Vec\"\nreason = \"test\"\n",
        )
        .expect("write");
    let allow = load_allowlist(&path).expect("load");
    assert_eq!(allow.monomorph.len(), 1);
    assert_eq!(allow.monomorph.first().expect("row").reason, "test");
}
