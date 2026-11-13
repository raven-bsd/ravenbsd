// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn refusal_carries_exit_one() {
    let error = Failure::refused("exports mismatch");
    assert_eq!(exit_code(&error), 1);
    assert_eq!(format!("{error}"), "exports mismatch");
}

#[test]
fn missing_tool_is_a_skip_with_exit_77() {
    let error = Failure::skipped("cc");
    assert_eq!(exit_code(&error), 77);
    assert_eq!(format!("{error}"), "skipped: cc not found");
}

#[test]
fn ordinary_errors_are_usage_errors() {
    let error = anyhow::anyhow!("bad flag");
    assert_eq!(exit_code(&error), 2);
}
