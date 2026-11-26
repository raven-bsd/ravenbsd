// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for byte-path mount routing.

use super::*;

fn table() -> MountTable {
    let mut table = MountTable::new();
    table.mount("/", "initfs:").unwrap();
    table.mount("/tmp", "ramfs:").unwrap();
    table.mount("/tmp/nested", "nested:").unwrap();
    table
}

#[test]
fn root_mount_strips_the_leading_slash() {
    let table = table();
    assert_eq!(
        mount_suffix(&table, b"/usr/bin"),
        Ok((1, b"usr/bin".to_vec()))
    );
}

#[test]
fn mount_point_has_an_empty_suffix() {
    let table = table();
    assert_eq!(mount_suffix(&table, b"/tmp"), Ok((2, Vec::new())));
    assert_eq!(mount_suffix(&table, b"/"), Ok((1, Vec::new())));
}

#[test]
fn the_longest_mount_prefix_wins() {
    let table = table();
    assert_eq!(
        mount_suffix(&table, b"/tmp/nested/file"),
        Ok((3, b"file".to_vec()))
    );
    assert_eq!(
        mount_suffix(&table, b"/tmp/file"),
        Ok((2, b"file".to_vec()))
    );
}

#[test]
fn a_component_prefix_of_a_mount_name_is_not_a_mount() {
    let table = table();
    assert_eq!(
        mount_suffix(&table, b"/tmpx"),
        Ok((1, b"tmpx".to_vec()))
    );
}

#[test]
fn non_utf8_suffix_bytes_are_preserved() {
    let table = table();
    assert_eq!(
        mount_suffix(&table, b"/tmp/\xff\xfe"),
        Ok((2, b"\xff\xfe".to_vec()))
    );
}

#[test]
fn parent_bytes_drops_the_final_component() {
    assert_eq!(parent_bytes(b"a/b/c"), b"a/b");
    assert_eq!(parent_bytes(b"a"), b"");
    assert_eq!(parent_bytes(b""), b"");
    assert_eq!(parent_bytes(b"a/b/"), b"a");
}

#[test]
fn component_prefixes_walk_from_the_top() {
    let prefixes = component_prefixes(b"a/b/c");
    assert_eq!(prefixes, vec![&b"a"[..], &b"a/b"[..], &b"a/b/c"[..]]);
    assert_eq!(component_prefixes(b""), Vec::<&[u8]>::new());
    assert_eq!(component_prefixes(b"/"), Vec::<&[u8]>::new());
    assert_eq!(component_prefixes(b"a/"), vec![&b"a"[..]]);
}

#[test]
fn component_prefixes_keep_non_utf8_bytes() {
    assert_eq!(
        component_prefixes(b"\xff/b"),
        vec![&b"\xff"[..], &b"\xff/b"[..]]
    );
}
