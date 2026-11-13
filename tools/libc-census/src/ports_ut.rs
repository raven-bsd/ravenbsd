// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for snapshot layout discovery and patch aggregation.

use std::collections::BTreeMap;

use super::*;
use crate::testutil::fixture;

fn rings() -> BTreeMap<String, u8> {
    BTreeMap::from([
        ("strlcpy".to_string(), 1),
        ("epoll_create".to_string(), 2),
        ("memchr".to_string(), 0),
    ])
}

fn ids(packages: &[LocatedPackage]) -> Vec<&str> {
    packages.iter().map(|package| package.id.as_str()).collect()
}

#[test]
fn aports_layout_lists_packages_and_patches() {
    let packages = locate("aports", &fixture("aports")).unwrap();
    assert_eq!(ids(&packages), vec!["bar", "baz", "foo"]);
    let foo = packages.iter().find(|package| package.id == "foo").unwrap();
    assert_eq!(foo.patches.len(), 1);
    assert!(foo.patches[0].ends_with("fix-glibc.patch"));
}

#[test]
fn pkgsrc_layout_uses_category_package_ids() {
    let packages = locate("pkgsrc", &fixture("pkgsrc")).unwrap();
    assert_eq!(ids(&packages), vec!["devel/pkg1", "devel/pkg2"]);
    assert_eq!(packages[0].patches.len(), 1);
}

#[test]
fn freebsd_ports_layout_reads_files_patch_star() {
    let packages = locate("freebsd-ports", &fixture("freebsd-ports")).unwrap();
    assert_eq!(ids(&packages), vec!["devel/port1"]);
    assert_eq!(packages[0].patches.len(), 1);
    assert!(packages[0].patches[0].ends_with("files/patch-one"));
}

#[test]
fn openbsd_ports_layout_reads_patches_patch_star() {
    let packages = locate("openbsd-ports", &fixture("openbsd-ports")).unwrap();
    assert_eq!(ids(&packages), vec!["devel/port2"]);
    assert_eq!(packages[0].patches.len(), 1);
    assert!(packages[0].patches[0].ends_with("patches/patch-two"));
}

#[test]
fn unknown_snapshot_is_an_error() {
    assert!(locate("not-a-snapshot", &fixture("aports")).is_err());
}

#[test]
fn a_missing_snapshot_is_skipped_not_an_error() {
    let output = scan(&PortsOptions::default(), &rings()).unwrap();
    assert_eq!(output.snapshots.len(), 4);
    assert!(output.packages.is_empty());
    for summary in output.snapshots.values() {
        assert!(summary.skipped.as_deref().unwrap().starts_with("skipped:"));
    }
}

#[test]
fn a_directory_that_does_not_exist_is_skipped() {
    let output = scan(
        &PortsOptions {
            aports: Some(fixture("no-such-aports")),
            ..PortsOptions::default()
        },
        &rings(),
    )
    .unwrap();
    assert!(
        output.snapshots["aports"]
            .skipped
            .as_deref()
            .unwrap()
            .contains("missing")
    );
}

#[test]
fn scan_counts_packages_not_patches() {
    let output = scan(
        &PortsOptions {
            aports: Some(fixture("aports")),
            ..PortsOptions::default()
        },
        &rings(),
    )
    .unwrap();
    assert_eq!(output.snapshots["aports"].packages, 3);
    assert_eq!(output.symbols["strlcpy"], 1);
    assert_eq!(output.symbols["epoll_create"], 1);
    assert_eq!(output.headers["sys/epoll.h"], 1);
    assert_eq!(output.headers["sys/queue.h"], 1);
    assert_eq!(output.macros["__GLIBC__"], 1);
    let foo = &output.packages["foo"];
    assert_eq!(foo.patches, 1);
    assert_eq!(foo.categories.len(), 3);
}

#[test]
fn scan_merges_package_counts_across_snapshots() {
    let output = scan(
        &PortsOptions {
            aports: Some(fixture("aports")),
            pkgsrc: Some(fixture("pkgsrc")),
            freebsd_ports: Some(fixture("freebsd-ports")),
            openbsd_ports: Some(fixture("openbsd-ports")),
        },
        &rings(),
    )
    .unwrap();
    assert_eq!(output.symbols["strlcpy"], 2);
    assert_eq!(output.headers["sys/queue.h"], 2);
    assert_eq!(output.macros["_GNU_SOURCE"], 1);
    assert_eq!(output.packages.len(), 7);
}
