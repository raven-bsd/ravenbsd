// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `svcctl` argument parsing.

use super::*;

#[test]
fn lists_services() {
    assert_eq!(parse(&["svcctl", "list"]), Ok(Command::List));
}

#[test]
fn parses_service_commands() {
    assert_eq!(
        parse(&["svcctl", "status", "logd"]),
        Ok(Command::Status("logd".to_string()))
    );
    assert_eq!(
        parse(&["svcctl", "start", "logd"]),
        Ok(Command::Start("logd".to_string()))
    );
    assert_eq!(
        parse(&["svcctl", "stop", "logd"]),
        Ok(Command::Stop("logd".to_string()))
    );
    assert_eq!(
        parse(&["svcctl", "restart", "logd"]),
        Ok(Command::Restart("logd".to_string()))
    );
}

#[test]
fn rejects_bad_argument_counts() {
    assert!(parse(&[]).is_err());
    assert!(parse(&["svcctl"]).is_err());
    assert!(parse(&["svcctl", "status"]).is_err());
    assert!(parse(&["svcctl", "status", "a", "b"]).is_err());
    assert!(parse(&["svcctl", "frobnicate"]).is_err());
}

#[test]
fn usage_is_documented_in_the_error() {
    let error = parse(&["svcctl", "nope"]).unwrap_err();
    assert!(error.contains("svcctl list"));
    assert!(error.contains("restart <name>"));
}
