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

//! `dirname` vectors from NetBSD `t_basedirname.c`.

use super::*;

fn name(path: &[u8]) -> Vec<u8> {
    match dirname(path) {
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
        (b"a", b"."),
        (b"/", b"/"),
        (b"//", b"/"),
        (b"///", b"/"),
        (b"/a", b"/"),
        (b"a/b", b"a"),
        (b"a/b/", b"a"),
        (b"a//b", b"a"),
        (b"/a/b", b"/a"),
        (b".", b"."),
        (b"..", b"."),
        (b"/usr/bin/", b"/usr"),
    ];
    for (path, expected) in cases {
        assert_eq!(name(path), *expected, "path {path:?}");
    }
}

#[test]
fn no_slash_is_the_static_dot() {
    assert_eq!(dirname(b"abc"), GenName::Static(b"."));
}

#[test]
fn all_slashes() {
    assert_eq!(name(b"////"), b"/");
}

#[test]
fn nested_relative() {
    assert_eq!(name(b"a/b/c"), b"a/b");
    assert_eq!(name(b"./a"), b".");
    assert_eq!(name(b"../a"), b"..");
}
