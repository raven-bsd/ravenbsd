// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the manifest loader.

use super::*;

#[test]
fn parses_a_minimal_manifest() {
    let rows = parse(
        r#"
[[header]]
path = "stdio.h"
owner = "C37"
status = "owned"
standalone = true
ring = 1
[[header]]
path = "signal.h"
owner = "C38"
status = "pending"
standalone = true
ring = 1
"#,
    )
    .expect("parse");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, Status::Owned);
    assert_eq!(rows[1].status, Status::Pending);
    assert_eq!(owned_standalone(&rows), vec!["stdio.h"]);
}

#[test]
fn duplicate_header_is_rejected() {
    let text = r#"
[[header]]
path = "stdio.h"
owner = "C37"
status = "owned"
standalone = true
ring = 1
[[header]]
path = "stdio.h"
owner = "C38"
status = "pending"
standalone = true
ring = 1
"#;
    let error = parse(text).expect_err("duplicate");
    assert!(error.contains("duplicate"), "{error}");
}

#[test]
fn real_manifest_has_at_least_190_rows() {
    let rows = load().expect("manifest");
    assert!(rows.len() >= 190, "only {} rows", rows.len());
    assert!(rows.iter().any(|row| row.path == "stdio.h"));
    assert!(rows.iter().any(|row| row.path == "bits/errno.h"));
}

#[test]
fn disk_headers_are_install_names() {
    let names = disk_headers().expect("disk");
    assert!(names.contains(&"stdio.h".to_string()));
    assert!(names.contains(&"bits/alltypes.h".to_string()));
    assert!(!names.iter().any(|name| name.starts_with("arch/")));
    assert!(!names.iter().any(|name| name.starts_with("include/")));
}

#[test]
fn owner_filter_matches() {
    let row = HeaderRow {
        path: "signal.h".to_string(),
        owner: "C38".to_string(),
        status: Status::Pending,
        standalone: true,
        ring: 1,
    };
    assert!(row.owned_by(&["C37".to_string(), "C38".to_string()]));
    assert!(!row.owned_by(&["C37".to_string()]));
    assert!(pending_allowed("C38", &["C37".to_string()], false));
    assert!(!pending_allowed("C37", &["C37".to_string()], false));
    assert!(!pending_allowed("C38", &[], true));
}
