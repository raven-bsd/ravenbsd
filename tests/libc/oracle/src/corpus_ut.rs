// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-libc-oracle-corpus-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).expect("temp dir");
    path
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Row {
    n: u64,
}

#[test]
fn text_and_hex_bytes_are_accepted() {
    assert_eq!(
        parse_bytes(Some("hello".to_string()), None, "case").unwrap(),
        b"hello"
    );
    assert_eq!(
        parse_bytes(None, Some("00ff80".to_string()), "case").unwrap(),
        vec![0x00, 0xff, 0x80]
    );
}

#[test]
fn bad_byte_fields_are_errors() {
    assert!(parse_bytes(Some("a\u{0}".to_string()), None, "case").is_err());
    assert!(parse_bytes(Some("é".to_string()), None, "case").is_err());
    assert!(parse_bytes(None, Some("abc".to_string()), "case").is_err());
    assert!(parse_bytes(None, Some("zz".to_string()), "case").is_err());
    assert!(parse_bytes(Some("a".to_string()), Some("61".to_string()), "case").is_err());
    assert!(parse_bytes(None, None, "case").is_err());
}

#[test]
fn render_round_trips_through_parse() {
    let bytes = vec![0x00, 0x7f, 0x80, 0xff];
    let rendered = render_bytes(&bytes);
    let hex = rendered
        .strip_prefix("bytes_hex = \"")
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("line");
    assert_eq!(
        parse_bytes(None, Some(hex.to_string()), "case").unwrap(),
        bytes
    );
}

#[test]
fn a_corpus_file_loads_and_an_empty_one_panics() {
    let base = temp_dir("load");
    fs::write(base.join("one.toml"), "[[case]]\nn = 7\n").unwrap();
    let rows: Vec<Row> = load_from(&base, "one.toml");
    assert_eq!(rows, vec![Row { n: 7 }]);
    fs::write(base.join("empty.toml"), "").unwrap();
    let result = std::panic::catch_unwind(|| {
        let _: Vec<Row> = load_from(&base, "empty.toml");
    });
    assert!(result.is_err());
    let result = std::panic::catch_unwind(|| {
        let _: Vec<Row> = load_from(&base, "../one.toml");
    });
    assert!(result.is_err());
    let _ = fs::remove_dir_all(base);
}

#[test]
fn an_unknown_key_is_an_error() {
    let base = temp_dir("unknown");
    fs::write(base.join("bad.toml"), "[[case]]\nn = 7\nm = 8\n").unwrap();
    let result = std::panic::catch_unwind(|| {
        let _: Vec<Row> = load_from(&base, "bad.toml");
    });
    assert!(result.is_err());
    let _ = fs::remove_dir_all(base);
}
