// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for `toml_lite`.

use super::*;

#[test]
fn parses_tables_and_values() {
    let doc = parse(
        r#"
# comment
[[header]]
path = "stdio.h" # trailing
owner = "C37"
standalone = true
ring = 1
arches = ["x86_64", "aarch64"]

[[header]]
path = "bits/alltypes.h"
owner = "C37"
standalone = false
ring = 1
"#,
    )
    .expect("parse");
    let rows = doc.get("header");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get_str("path").unwrap(), "stdio.h");
    assert!(rows[0].get_bool("standalone").unwrap());
    assert_eq!(rows[0].get_int("ring").unwrap(), 1);
    assert_eq!(
        rows[0].get_str_array("arches").unwrap(),
        vec!["x86_64", "aarch64"]
    );
    assert!(!rows[1].get_bool("standalone").unwrap());
}

#[test]
fn parses_subtables() {
    let doc = parse(
        r#"
[[type]]
name = "struct stat"
size = 128
[[type.field]]
name = "st_dev"
offset = 0
bytes = 8
[[type.field]]
name = "st_ino"
offset = 8
"#,
    )
    .expect("parse");
    let rows = doc.get("type");
    assert_eq!(rows.len(), 1);
    let fields = rows[0].get_tables("field");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].get_str("name").unwrap(), "st_dev");
    assert_eq!(fields[0].get_int("offset").unwrap(), 0);
    assert_eq!(fields[0].opt_int("bytes").unwrap(), Some(8));
    assert_eq!(fields[1].opt_int("bytes").unwrap(), None);
}

#[test]
fn rejects_unknown_value() {
    let error = parse("[[x]]\nkey = bare\n").expect_err("must fail");
    assert!(error.contains("unsupported value"), "{error}");
}

#[test]
fn comment_marker_inside_string_is_kept() {
    let doc = parse("[[x]]\nname = \"a#b\"\n").expect("parse");
    assert_eq!(doc.get("x")[0].get_str("name").unwrap(), "a#b");
}

#[test]
fn empty_document_is_valid() {
    let doc = parse("# nothing\n").expect("parse");
    assert!(doc.arrays.is_empty());
}
