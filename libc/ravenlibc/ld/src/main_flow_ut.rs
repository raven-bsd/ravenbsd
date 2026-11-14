// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn split_paths_keeps_the_order() {
    let mut out = [Name::empty(); 8];
    let count = split_paths(b"/lib:/usr/lib::/x", &mut out);
    assert_eq!(count, 4);
    assert_eq!(out[0].as_bytes(), b"/lib");
    assert_eq!(out[1].as_bytes(), b"/usr/lib");
    assert_eq!(out[2].as_bytes(), b"");
    assert_eq!(out[3].as_bytes(), b"/x");
}

#[test]
fn split_paths_respects_the_array_bound() {
    let mut out = [Name::empty(); 1];
    assert_eq!(split_paths(b"a:b:c", &mut out), 1);
}

#[test]
fn the_main_id_is_zero() {
    assert_eq!(main_id(), ObjId(0));
}
