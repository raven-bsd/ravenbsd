// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `rm`.

use super::{Action, action, missing, parse};
use raven_tool_core::Errno;

#[test]
fn directory_without_recursion_is_eisdir() {
    assert_eq!(action(true, false), Err(Errno::EISDIR));
}

#[test]
fn directory_with_recursion_is_removed_bottom_up() {
    assert_eq!(action(true, true), Ok(Action::RemoveDir));
}

#[test]
fn file_is_unlinked() {
    assert_eq!(action(false, false), Ok(Action::Unlink));
}

#[test]
fn missing_with_force_is_ignored() {
    assert_eq!(missing(true), Ok(Action::Ignore));
}

#[test]
fn missing_without_force_is_enoent() {
    assert_eq!(missing(false), Err(Errno::ENOENT));
}

#[test]
fn recursive_and_force_flags_are_parsed() {
    let parsed = parse(["-rf"]).unwrap();
    assert!(parsed.flags.r && parsed.flags.f);
}

#[test]
fn operands_are_kept() {
    assert_eq!(parse(["a", "b"]).unwrap().operands, vec!["a", "b"]);
}

#[test]
fn unknown_flag_is_rejected() {
    assert!(parse(["-z"]).is_err());
}

#[test]
fn options_after_double_dash_are_operands() {
    assert_eq!(parse(["--", "-r"]).unwrap().operands, vec!["-r"]);
}
