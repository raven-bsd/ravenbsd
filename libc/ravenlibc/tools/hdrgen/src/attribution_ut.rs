// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for donor-aware table attribution.

use super::*;
use crate::model::{ConstFile, DonorSpec};
use crate::provenance::CommentStyle;

fn lock() -> Lock {
    Lock {
        musl: "musl-rev".to_string(),
        linux_raw_sys: "linux-rev".to_string(),
        freebsd: "freebsd-rev".to_string(),
    }
}

fn group(source: &[&str], donor: Option<DonorSpec>) -> ConstFile {
    ConstFile {
        schema: 1,
        group: "probe".to_string(),
        c_header: "bits/probe.h".to_string(),
        guard: "_PROBE".to_string(),
        rust_type: "i32".to_string(),
        base: "dec".to_string(),
        source: source.iter().map(|value| (*value).to_string()).collect(),
        donor,
        prefixes: Vec::new(),
        omit: Vec::new(),
        consts: Vec::new(),
    }
}

fn freebsd_lease() -> DonorSpec {
    DonorSpec {
        name: "freebsd".to_string(),
        revision: "freebsd-rev".to_string(),
        license: "BSD-2-Clause".to_string(),
    }
}

#[test]
fn a_bare_source_stays_musl() {
    let attribution = resolve_group(&group(&["include/errno.h"], None), &lock()).expect("resolve");
    assert_eq!(attribution.donor, "musl");
    assert_eq!(attribution.revision, "musl-rev");
    assert!(
        attribution
            .notice(CommentStyle::LineSlash)
            .contains("musl:include/errno.h")
    );
}

#[test]
fn a_freebsd_table_uses_its_reviewed_source() {
    let def = group(&["freebsd:sys/sys/errno.h"], Some(freebsd_lease()));
    let attribution = resolve_group(&def, &lock()).expect("resolve");
    assert_eq!(attribution.donor, "freebsd");
    assert_eq!(attribution.paths, vec!["sys/sys/errno.h".to_string()]);
    let notice = attribution.notice(CommentStyle::BlockC);
    assert!(
        notice.contains("SPDX-License-Identifier: BSD-2-Clause"),
        "{notice}"
    );
    assert!(
        notice.contains("freebsd:sys/sys/errno.h @ freebsd-rev"),
        "{notice}"
    );
    assert!(!notice.contains("musl"), "{notice}");
}

#[test]
fn a_freebsd_source_without_a_lease_is_rejected() {
    let error = resolve_group(&group(&["freebsd:sys/sys/errno.h"], None), &lock())
        .expect_err("must reject");
    assert!(error.to_string().contains("no donor lease"), "{error}");
    assert!(
        error.to_string().contains("refusing to label it musl"),
        "{error}"
    );
}

#[test]
fn a_freebsd_lease_with_musl_sources_is_rejected() {
    let error = resolve_group(&group(&["include/errno.h"], Some(freebsd_lease())), &lock())
        .expect_err("must reject");
    assert!(
        error.to_string().contains("cannot be attributed to musl"),
        "{error}"
    );
}

#[test]
fn a_wrong_revision_is_rejected() {
    let mut spec = freebsd_lease();
    spec.revision = "other".to_string();
    let error = resolve_group(&group(&["freebsd:sys/sys/errno.h"], Some(spec)), &lock())
        .expect_err("must reject");
    assert!(
        error.to_string().contains("donors.lock pins freebsd-rev"),
        "{error}"
    );
}

#[test]
fn an_unknown_donor_prefix_is_rejected() {
    let error =
        resolve_group(&group(&["plan9:sys/errno.h"], None), &lock()).expect_err("must reject");
    assert!(
        error.to_string().contains("unknown donor prefix"),
        "{error}"
    );
}

#[test]
fn mixed_donors_are_rejected() {
    let error = resolve_group(
        &group(
            &["include/errno.h", "freebsd:sys/sys/errno.h"],
            Some(freebsd_lease()),
        ),
        &lock(),
    )
    .expect_err("must reject");
    assert!(error.to_string().contains("mixes donor sources"), "{error}");
}

#[test]
fn a_licence_outside_the_allow_list_is_rejected() {
    let mut spec = freebsd_lease();
    spec.license = "GPL-2.0".to_string();
    let error = resolve_group(&group(&["freebsd:sys/sys/errno.h"], Some(spec)), &lock())
        .expect_err("must reject");
    assert!(
        error.to_string().contains("outside the donor allow-list"),
        "{error}"
    );
}

#[test]
fn an_empty_freebsd_source_list_is_rejected() {
    let error = resolve_group(&group(&[], Some(freebsd_lease())), &lock()).expect_err("reject");
    assert!(
        error.to_string().contains("lists no freebsd:<path> source"),
        "{error}"
    );
}
