// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Readiness-timeout limit tests (K96).

use super::*;

fn parse(text: &str) -> Manifest {
    Manifest::parse(text).expect("manifest should parse")
}

#[test]
fn the_default_remains_five_seconds() {
    let manifest = parse("name = \"x\"\nexec = \"/x\"");
    assert_eq!(manifest.ready_timeout_ms, 5_000);
}

#[test]
fn the_maximum_is_five_minutes() {
    let manifest = parse("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = 300_000");
    assert_eq!(manifest.ready_timeout_ms, MAX_READY_TIMEOUT_MS);
    let manifest = parse("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = 120_000");
    assert_eq!(manifest.ready_timeout_ms, 120_000);
}

#[test]
fn one_above_the_maximum_is_rejected() {
    assert!(matches!(
        Manifest::parse("name = \"x\"\nexec = \"/x\"\nready_timeout_ms = 300001")
            .expect_err("out of range")
            .kind,
        ParseErrorKind::IntegerOutOfRange("ready_timeout_ms")
    ));
}

#[test]
fn the_large_image_manifests_have_headroom() {
    let initfsd = parse(include_str!("../../../etc/services.d/initfsd.toml"));
    assert_eq!(initfsd.ready_timeout_ms, 120_000);
    let vfsd = parse(include_str!("../../../etc/services.d/vfsd.toml"));
    assert_eq!(vfsd.ready_timeout_ms, 60_000);
}

#[test]
fn file_readiness_parses_the_file_and_match() {
    let manifest = parse(
        "name = \"sshd\"\nexec = \"/sbin/sshd\"\nreadiness = \"file\"\nready_file = \"/tmp/sshd.log\"\nready_match = \"Server listening on 0.0.0.0 port 22.\"",
    );
    assert_eq!(manifest.readiness, Readiness::File);
    assert_eq!(manifest.ready_file, "/tmp/sshd.log");
    assert_eq!(manifest.ready_match, "Server listening on 0.0.0.0 port 22.");
}

#[test]
fn file_readiness_requires_the_file() {
    assert!(matches!(
        Manifest::parse(
            "name = \"sshd\"\nexec = \"/sbin/sshd\"\nreadiness = \"file\"\nready_match = \"x\""
        )
        .expect_err("missing ready_file")
        .kind,
        ParseErrorKind::MissingKey("ready_file")
    ));
}

#[test]
fn file_readiness_requires_the_match() {
    assert!(matches!(
        Manifest::parse(
            "name = \"sshd\"\nexec = \"/sbin/sshd\"\nreadiness = \"file\"\nready_file = \"/tmp/x\""
        )
        .expect_err("missing ready_match")
        .kind,
        ParseErrorKind::MissingKey("ready_match")
    ));
}

#[test]
fn ready_file_must_be_absolute() {
    assert!(matches!(
        Manifest::parse(
            "name = \"sshd\"\nexec = \"/sbin/sshd\"\nreadiness = \"file\"\nready_file = \"tmp/x\"\nready_match = \"x\""
        )
        .expect_err("relative ready_file")
        .kind,
        ParseErrorKind::InvalidValue("ready_file")
    ));
}
