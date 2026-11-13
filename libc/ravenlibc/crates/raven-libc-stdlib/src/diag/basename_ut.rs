// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `basename` vectors from NetBSD `t_basedirname.c`.

use super::*;

fn name(path: &[u8]) -> Vec<u8> {
    match basename(path) {
        GenName::Static(text) => text.to_vec(),
        GenName::Slice { start, len } => path
            .get(start..start.wrapping_add(len))
            .unwrap_or(&[])
            .to_vec(),
    }
}

#[test]
fn posix_vectors() {
    let cases: &[(&[u8], &[u8])] = &[
        (b"", b"."),
        (b"/", b"/"),
        (b"//", b"/"),
        (b"///", b"/"),
        (b"a/", b"a"),
        (b"a//", b"a"),
        (b"/a", b"a"),
        (b"a/b", b"b"),
        (b"a//b//", b"b"),
        (b".", b"."),
        (b"..", b".."),
        (b"/usr/bin/", b"bin"),
    ];
    for (path, expected) in cases {
        assert_eq!(name(path), *expected, "path {path:?}");
    }
}

#[test]
fn empty_is_the_static_dot() {
    assert_eq!(basename(b""), GenName::Static(b"."));
}

#[test]
fn all_slashes() {
    assert_eq!(name(b"////"), b"/");
}

#[test]
fn dots_and_slashes() {
    assert_eq!(name(b"./"), b".");
    assert_eq!(name(b"../"), b"..");
    assert_eq!(name(b"a/./"), b".");
}
