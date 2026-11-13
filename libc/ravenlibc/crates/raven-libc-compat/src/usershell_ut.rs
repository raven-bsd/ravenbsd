// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `usershell` tests.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use crate::testutil::FakeFiles;

fn load(contents: Option<&[u8]>) -> Shells {
    let mut files = FakeFiles::new();
    if let Some(contents) = contents {
        files.insert(PATH_SHELLS, contents);
    }
    Shells::load(&mut files)
}

#[test]
fn parses_slash_lines_and_skips_others() {
    let shells = load(Some(b"# comment\n/bin/sh\n\nnot-a-shell\n/usr/bin/csh\n"));
    assert_eq!(
        shells.entries(),
        &[b"/bin/sh".to_vec(), b"/usr/bin/csh".to_vec()]
    );
}

#[test]
fn cuts_at_a_hash_and_newline() {
    let shells = load(Some(b"/bin/sh # the shell\n/bin/ksh#alt\n"));
    assert_eq!(
        shells.entries(),
        &[b"/bin/sh ".to_vec(), b"/bin/ksh".to_vec()]
    );
}

#[test]
fn missing_file_falls_back_to_bin_sh() {
    let shells = load(None);
    assert_eq!(shells.entries(), &[b"/bin/sh".to_vec()]);
}

#[test]
fn empty_file_falls_back_to_bin_sh() {
    let shells = load(Some(b"# nothing\n"));
    assert_eq!(shells.entries(), &[b"/bin/sh".to_vec()]);
}

#[test]
fn next_walks_then_returns_none() {
    let mut shells = load(Some(b"/bin/sh\n/bin/csh\n"));
    assert_eq!(shells.next().unwrap(), b"/bin/sh");
    assert_eq!(shells.next().unwrap(), b"/bin/csh");
    assert!(shells.next().is_none());
}

#[test]
fn rewind_restarts() {
    let mut shells = load(Some(b"/bin/sh\n/bin/csh\n"));
    assert!(shells.next().is_some());
    shells.rewind();
    assert_eq!(shells.next().unwrap(), b"/bin/sh");
}

#[test]
fn last_line_without_newline() {
    let shells = load(Some(b"/bin/sh"));
    assert_eq!(shells.entries(), &[b"/bin/sh".to_vec()]);
}

#[test]
fn a_lone_slash_is_kept() {
    let shells = load(Some(b"/\n"));
    assert_eq!(shells.entries(), &[b"/".to_vec()]);
}
