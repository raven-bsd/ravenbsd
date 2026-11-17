// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the mailx front end.

use super::*;

#[test]
fn the_tool_name_is_mailx() {
    assert_eq!(TOOL, "mailx");
}

#[test]
fn the_front_end_uses_the_shared_option_parser() {
    let arguments = vec!["-e".to_string()];
    let invocation = mail::parse_args(TOOL, &arguments).expect("parses");
    assert_eq!(invocation.tool, TOOL);
    assert_eq!(invocation.action, mail::Action::Exists);
}
