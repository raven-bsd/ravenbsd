// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host checks for the `talk` command line and local loopback diagnostic.

use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_talk")
}

#[test]
fn loopback_runs_both_directions() {
    let output = Command::new(binary()).arg("--loopback").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("invitation: Message from local"));
    assert!(stdout.contains("screen: split"));
    assert!(stdout.contains("caller> hello"));
    assert!(stdout.contains("peer< hello"));
    assert!(stdout.contains("peer> hi"));
    assert!(stdout.contains("caller< hi"));
    assert!(stdout.contains("local loopback closed (caller=0 peer=0)"));
}

#[test]
fn missing_address_is_a_usage_error() {
    let output = Command::new(binary()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: talk"));
}

#[test]
fn network_address_is_documented_as_unsupported() {
    let output = Command::new(binary()).arg("alice@remote").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("network talk to alice@remote is not implemented")
    );
}

#[test]
fn local_address_reports_the_missing_registry() {
    let output = Command::new(binary()).arg("alice").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no logged-in terminal for alice"));
}

#[test]
fn listener_reports_the_missing_transport() {
    let output = Command::new(binary()).arg("-l").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no peer transport for a local listener")
    );
}
