// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_alias_table_maps_every_ring_two_name() {
    for alias in ALIASES {
        assert_eq!(canonical(alias), b"libc.so.1");
    }
}

#[test]
fn an_ordinary_name_is_unchanged() {
    assert_eq!(canonical(b"libfoo.so.1"), b"libfoo.so.1");
}

#[test]
fn join_inserts_one_slash() {
    let mut out = [0u8; 64];
    let length = join(b"/lib", b"libc.so.1", &mut out).expect("join");
    assert_eq!(&out[..length], b"/lib/libc.so.1");
}

#[test]
fn join_refuses_an_empty_directory() {
    let mut out = [0u8; 8];
    assert!(join(b"", b"x", &mut out).is_none());
}

#[test]
fn absolute_and_slash_checks_work() {
    assert!(is_absolute(b"/lib/x.so"));
    assert!(!is_absolute(b"lib/x.so"));
    assert!(has_slash(b"lib/x.so"));
    assert!(!has_slash(b"libx.so"));
}

#[test]
fn origin_is_detected() {
    assert!(has_origin(b"$ORIGIN/lib"));
    assert!(has_origin(b"/x/$ORIGIN/y"));
    assert!(!has_origin(b"/x/origin/y"));
    assert!(!has_origin(b""));
}
