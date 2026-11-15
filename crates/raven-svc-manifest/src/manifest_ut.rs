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
