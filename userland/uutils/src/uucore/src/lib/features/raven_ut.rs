// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared RavenBSD output tests.

use raven_output::{Record, Value};
use raven_theme::Decision;

#[test]
fn json_record_has_the_schema_and_kind_envelope() {
    let record = Record::new("file")
        .string("path", "/tmp/a")
        .u64("size", 4)
        .bool("directory", false);
    assert_eq!(
        record.to_json(),
        r#"{"schema":1,"kind":"file","path":"/tmp/a","size":4,"directory":false}"#
    );
}

#[test]
fn json_value_escapes_the_path() {
    let record = Record::new("file").string("path", "a\"b");
    assert!(record.to_json().contains(r#""path":"a\"b""#));
}

#[test]
fn color_decision_honours_the_contract() {
    assert!(Decision::Auto.enabled(true, false, false, false));
    assert!(!Decision::Auto.enabled(false, false, false, false));
    assert!(!Decision::Auto.enabled(true, true, false, false));
    assert!(!Decision::Auto.enabled(true, false, true, false));
    assert!(Decision::Always.enabled(false, true, true, false));
    assert!(!Decision::Never.enabled(true, false, false, false));
    assert!(!Decision::Always.enabled(true, false, false, true));
}

#[test]
fn default_ls_colors_uses_the_raven_palette() {
    let value = super::default_ls_colors();
    assert!(value.contains("di=38;5;"));
    assert!(!value.contains('\u{1b}'));
}

#[test]
fn json_values_keep_their_types() {
    let record = Record::new("wc")
        .u64("bytes", 6)
        .i64("delta", -1)
        .field("present", Value::Null);
    assert_eq!(
        record.to_json(),
        r#"{"schema":1,"kind":"wc","bytes":6,"delta":-1,"present":null}"#
    );
}
