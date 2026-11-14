// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden tests for the NDJSON contract.

use raven_output::{Color, ErrorEnvelope, Exit, JsonWriter, Record, SCHEMA, Value, escaped};

#[test]
fn escapes_the_json_controls() {
    assert_eq!(escaped("\""), "\\\"");
    assert_eq!(escaped("\\"), "\\\\");
    assert_eq!(escaped("\u{8}\u{c}\n\r\t"), "\\b\\f\\n\\r\\t");
    assert_eq!(escaped("\u{1}\u{1f}\u{7f}"), "\\u0001\\u001f\\u007f");
    assert_eq!(escaped("héllo/世界"), "héllo/世界");
}

#[test]
fn one_record_has_the_schema_and_kind_first() {
    let record = Record::new("service")
        .string("name", "netd")
        .u64("pid", 12)
        .bool("ready", true)
        .null("note");
    assert_eq!(
        record.to_json(),
        r#"{"schema":1,"kind":"service","name":"netd","pid":12,"ready":true,"note":null}"#
    );
    assert_eq!(SCHEMA, 1);
}

#[test]
fn escapes_field_names_too() {
    let record = Record::new("x").string("bad\"key", "v\n");
    assert_eq!(
        record.to_json(),
        r#"{"schema":1,"kind":"x","bad\"key":"v\n"}"#
    );
}

#[test]
fn arrays_and_objects_keep_order() {
    let record = Record::new("x")
        .array(
            "items",
            vec![Value::Unsigned(1), Value::String(String::from("a\"b"))],
        )
        .object("meta", vec![(String::from("k"), Value::Bool(false))]);
    assert_eq!(
        record.to_json(),
        r#"{"schema":1,"kind":"x","items":[1,"a\"b"],"meta":{"k":false}}"#
    );
}

#[test]
fn stream_framing_is_one_object_per_line() {
    let first = Record::new("a").u64("n", 1);
    let second = Record::new("b").u64("n", 2);
    let mut output = String::new();
    {
        let mut writer = JsonWriter::new(&mut output);
        writer.write_record(&first).unwrap();
        writer.write_record(&second).unwrap();
    }
    assert_eq!(
        output,
        "{\"schema\":1,\"kind\":\"a\",\"n\":1}\n{\"schema\":1,\"kind\":\"b\",\"n\":2}\n"
    );
    assert_eq!(output.lines().count(), 2);
    assert!(!output.starts_with('['));
    assert!(output.ends_with('\n'));
}

#[test]
fn the_error_object_is_stable() {
    let error = ErrorEnvelope::new("ENOENT", "service not found").with_context("name", "fo\"o");
    assert_eq!(
        error.to_json(),
        r#"{"schema":1,"kind":"error","code":"ENOENT","message":"service not found","context":{"name":"fo\"o"}}"#
    );
    assert_eq!(error.code(), "ENOENT");
    assert_eq!(error.message(), "service not found");
    assert_eq!(error.context().len(), 1);
}

#[test]
fn the_error_line_has_one_newline() {
    let error = ErrorEnvelope::new("EUSAGE", "bad input");
    let mut output = String::new();
    error.write_line(&mut output).unwrap();
    assert_eq!(
        output,
        "{\"schema\":1,\"kind\":\"error\",\"code\":\"EUSAGE\",\"message\":\"bad input\",\"context\":{}}\n"
    );
}

#[test]
fn exit_codes_are_zero_one_two() {
    assert_eq!(Exit::Success.code(), 0);
    assert_eq!(Exit::Failure.code(), 1);
    assert_eq!(Exit::Usage.code(), 2);
}

#[test]
fn colour_is_human_only_and_honours_no_color() {
    assert!(Color::Auto.enabled(true, false));
    assert!(!Color::Auto.enabled(false, false));
    assert!(!Color::Auto.enabled(true, true));
    assert!(Color::Always.enabled(false, true));
    assert!(!Color::Never.enabled(true, false));
    assert_eq!(Color::parse("auto"), Some(Color::Auto));
    assert_eq!(Color::parse("always"), Some(Color::Always));
    assert_eq!(Color::parse("never"), Some(Color::Never));
    assert_eq!(Color::parse("sometimes"), None);
}
