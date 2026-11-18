// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the post-overlay header reconciliation.

use super::*;

fn meta(id: &str, status: Status) -> Meta {
    Meta {
        id: id.to_string(),
        standard: STANDARD.to_string(),
        status,
        evidence: "none".to_string(),
        owner: "P1".to_string(),
        waiver: String::new(),
        notes: String::new(),
        pg: Vec::new(),
    }
}

fn interface(name: &str, status: Status) -> InterfaceRow {
    InterfaceRow {
        meta: meta(&format!("interface.{name}"), status),
        name: name.to_string(),
        header: "probe.h".to_string(),
        kind: "func".to_string(),
        required: true,
        signature: String::new(),
    }
}

fn header(id: &str, status: Status, functions: &[&str]) -> HeaderRow {
    HeaderRow {
        meta: meta(id, status),
        name: "probe.h".to_string(),
        required_macros: Vec::new(),
        required_types: Vec::new(),
        required_functions: functions.iter().map(|value| (*value).to_string()).collect(),
    }
}

fn profile(headers: Vec<HeaderRow>, interfaces: Vec<InterfaceRow>) -> Profile {
    Profile {
        schema: 1,
        standard: STANDARD.to_string(),
        platform: crate::model::PlatformStatus::default(),
        header: headers,
        interface: interfaces,
        utility: Vec::new(),
        shell_feature: Vec::new(),
        filesystem: Vec::new(),
        process: Vec::new(),
        tty: Vec::new(),
    }
}

#[test]
fn all_required_interfaces_present_or_waived_makes_the_header_present() {
    let mut profile = profile(
        vec![header(
            "header.probe.h",
            Status::Partial,
            &["first", "second"],
        )],
        vec![
            interface("first", Status::Present),
            interface("second", Status::Waived),
        ],
    );
    reconcile_header_status(&mut profile, &BTreeSet::new());
    assert_eq!(profile.header[0].meta.status, Status::Present);
}

#[test]
fn a_missing_required_interface_keeps_the_header_partial() {
    let mut profile = profile(
        vec![header(
            "header.probe.h",
            Status::Present,
            &["first", "second"],
        )],
        vec![
            interface("first", Status::Present),
            interface("second", Status::Missing),
        ],
    );
    reconcile_header_status(&mut profile, &BTreeSet::new());
    assert_eq!(profile.header[0].meta.status, Status::Partial);
}

#[test]
fn an_explicit_overlay_claim_is_not_overwritten() {
    let mut profile = profile(
        vec![header("header.probe.h", Status::Waived, &["first"])],
        vec![interface("first", Status::Missing)],
    );
    let claimed = BTreeSet::from(["header.probe.h".to_string()]);
    reconcile_header_status(&mut profile, &claimed);
    assert_eq!(profile.header[0].meta.status, Status::Waived);
}

#[test]
fn a_missing_header_file_is_not_promoted() {
    let mut profile = profile(
        vec![header("header.gone.h", Status::Missing, &["first"])],
        vec![interface("first", Status::Present)],
    );
    reconcile_header_status(&mut profile, &BTreeSet::new());
    assert_eq!(profile.header[0].meta.status, Status::Missing);
}
