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

//! `ProgName` vectors from `setprogname.c`.

use super::*;

#[test]
fn keeps_the_basename() {
    let mut prog = ProgName::new();
    prog.set(b"/usr/bin/tool");
    assert_eq!(prog.get(), b"tool");
    prog.set(b"tool");
    assert_eq!(prog.get(), b"tool");
    prog.set(b"/a/b/");
    assert_eq!(prog.get(), b"");
    assert_eq!(prog.display(), b"?");
}

#[test]
fn unset_is_empty_and_displays_question_mark() {
    let prog = ProgName::new();
    assert_eq!(prog.get(), b"");
    assert_eq!(prog.display(), b"?");
    assert_eq!(ProgName::default(), prog);
}

#[test]
fn display_uses_the_set_name() {
    let mut prog = ProgName::new();
    prog.set(b"/bin/ls");
    assert_eq!(prog.display(), b"ls");
}

#[test]
fn slash_only() {
    let mut prog = ProgName::new();
    prog.set(b"/");
    assert_eq!(prog.get(), b"");
}

#[test]
fn relative_directories() {
    let mut prog = ProgName::new();
    prog.set(b"./tool");
    assert_eq!(prog.get(), b"tool");
    prog.set(b"../dir/tool");
    assert_eq!(prog.get(), b"tool");
}
