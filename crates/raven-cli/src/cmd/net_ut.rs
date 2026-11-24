// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `raven net` formatting, JSON and error paths.

use std::ffi::OsString;

use raven_output::{Exit, Format};

use super::{Counters, Interface, NetBackend, Route, run_with_backend};
use crate::Context;

struct Fake;

impl NetBackend for Fake {
    fn interfaces(&self) -> Result<Vec<Interface>, super::BackendError> {
        Ok(vec![Interface {
            name: "eth0".to_string(),
            state: "up".to_string(),
            address: Some("10.0.2.15".to_string()),
            prefix: Some(24),
            mtu: 1500,
            mac: "52:54:00:12:34:56".to_string(),
            counters: Some(Counters {
                rx_packets: 7,
                tx_packets: 9,
                ..Counters::default()
            }),
        }])
    }

    fn routes(&self) -> Result<Vec<Route>, super::BackendError> {
        Ok(vec![
            Route {
                destination: "default".to_string(),
                gateway: Some("10.0.2.2".to_string()),
                interface: "eth0".to_string(),
                flags: vec!["UP".to_string(), "GATEWAY".to_string()],
            },
            Route {
                destination: "10.0.2.0/24".to_string(),
                gateway: None,
                interface: "eth0".to_string(),
                flags: vec!["UP".to_string()],
            },
        ])
    }
}

fn run(format: Format, args: &[&str]) -> (Exit, String, String) {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let exit = {
        let mut context = Context::new(format, false, &mut out, &mut err);
        run_with_backend(&mut context, &Fake, &args)
    };
    (
        exit,
        String::from_utf8(out).unwrap_or_default(),
        String::from_utf8(err).unwrap_or_default(),
    )
}

#[test]
fn if_table_shows_address_prefix_gateway_and_counters() {
    let (exit, out, _) = run(Format::Human, &["if"]);
    assert_eq!(exit, Exit::Success);
    assert!(out.contains("eth0"), "{out}");
    assert!(out.contains("10.0.2.15/24"), "{out}");
    assert!(out.contains("10.0.2.2"), "{out}");
    assert!(out.contains("7"), "{out}");
    assert!(out.contains("9"), "{out}");
}

#[test]
fn if_json_has_net_if_kind_and_the_live_address() {
    let (exit, out, _) = run(Format::Json, &["if"]);
    assert_eq!(exit, Exit::Success);
    let line = out.lines().next().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(line).expect("net.if json");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "net.if");
    assert_eq!(value["name"], "eth0");
    assert_eq!(value["state"], "up");
    assert_eq!(value["address"], "10.0.2.15");
    assert_eq!(value["prefix"], 24);
    assert_eq!(value["gateway"], "10.0.2.2");
    assert_eq!(value["counters"]["rx_packets"], 7);
}

#[test]
fn route_json_has_net_route_kind_and_the_gateway() {
    let (exit, out, _) = run(Format::Json, &["route"]);
    assert_eq!(exit, Exit::Success);
    let first = out.lines().next().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(first).expect("net.route json");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "net.route");
    assert_eq!(value["destination"], "default");
    assert_eq!(value["gateway"], "10.0.2.2");
    assert_eq!(value["interface"], "eth0");
}

#[test]
fn route_table_lists_the_default_gateway() {
    let (exit, out, _) = run(Format::Human, &["route"]);
    assert_eq!(exit, Exit::Success);
    assert!(out.contains("default"), "{out}");
    assert!(out.contains("10.0.2.2"), "{out}");
    assert!(out.contains("10.0.2.0/24"), "{out}");
}

#[test]
fn status_summarises_and_exits_zero() {
    let (exit, out, _) = run(Format::Human, &["status"]);
    assert_eq!(exit, Exit::Success);
    assert!(out.contains("interfaces: 1 (up: 1)"), "{out}");
    assert!(out.contains("default gateway: 10.0.2.2"), "{out}");
    assert!(out.contains("ready: yes"), "{out}");
}

#[test]
fn apply_is_reserved_for_k198_and_changes_nothing() {
    let (exit, _, err) = run(Format::Human, &["apply"]);
    assert_eq!(exit, Exit::Failure);
    assert!(err.contains("read-only"), "{err}");
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let (exit, _, err) = run(Format::Human, &["bogus"]);
    assert_eq!(exit, Exit::Usage);
    assert!(err.contains("net subcommand"), "{err}");
}

#[test]
fn missing_subcommand_is_a_usage_error() {
    let (exit, _, err) = run(Format::Human, &[]);
    assert_eq!(exit, Exit::Usage);
    assert!(err.contains("net needs"), "{err}");
}

#[test]
fn extra_arguments_are_rejected() {
    let (exit, _, err) = run(Format::Human, &["if", "extra"]);
    assert_eq!(exit, Exit::Usage);
    assert!(err.contains("takes no arguments"), "{err}");
}
