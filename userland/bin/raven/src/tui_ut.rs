// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the interactive invocation guard.

use std::ffi::OsString;

use super::request;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn config_on_a_tty_is_interactive() {
    let request = request(&args(&["config"]), true).expect("interactive");
    assert!(!request.dry_run);
}

#[test]
fn dry_run_is_preserved() {
    let request = request(&args(&["config", "--dry-run"]), true).expect("interactive");
    assert!(request.dry_run);
}

#[test]
fn config_subcommands_stay_scriptable() {
    assert!(request(&args(&["config", "check"]), true).is_none());
    assert!(request(&args(&["config", "show"]), true).is_none());
    assert!(request(&args(&["config", "get", "log.level"]), true).is_none());
}

#[test]
fn machine_flags_disable_the_tui() {
    assert!(request(&args(&["--json", "config"]), true).is_none());
    assert!(request(&args(&["config", "--quiet"]), true).is_none());
}

#[test]
fn non_tty_disables_the_tui() {
    assert!(request(&args(&["config"]), false).is_none());
}

#[test]
fn color_policy_is_honoured() {
    let parsed = request(&args(&["--color", "never", "config"]), true).expect("interactive");
    assert!(!parsed.color);
    let parsed = request(&args(&["--color=always", "config"]), true).expect("interactive");
    assert!(parsed.color);
}
