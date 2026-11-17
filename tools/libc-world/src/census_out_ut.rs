// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::Path;

use super::*;
use crate::testutil::TempDir;

#[test]
fn the_path_is_under_the_arch_and_undef_directory() {
    assert_eq!(
        undef_path(Path::new("/r"), "x86_64", "zlib"),
        Path::new("/r/x86_64/undef/zlib.undef")
    );
}

#[test]
fn the_wire_text_is_sorted_and_tab_separated() {
    let mut map = BTreeMap::new();
    map.insert("b".to_string(), 'w');
    map.insert("a".to_string(), 'U');
    assert_eq!(undef_text(&map), "a\tU\nb\tw\n");
}

#[test]
fn a_write_round_trips() {
    let temp = TempDir::new("census-round");
    let mut map = BTreeMap::new();
    map.insert("strlcpy".to_string(), 'U');
    map.insert("__cxa_finalize".to_string(), 'w');
    let path = write_undef(temp.path(), "x86_64", "zlib", &map).expect("write");
    assert!(path.is_file());
    assert_eq!(read_undef(&path).expect("read"), map);
}

#[test]
fn an_empty_map_writes_an_empty_file() {
    let temp = TempDir::new("census-empty");
    let path = write_undef(temp.path(), "x86_64", "zlib", &BTreeMap::new()).expect("write");
    assert_eq!(fs::read_to_string(path).expect("read"), "");
}

#[test]
fn parse_rejects_a_third_field() {
    let error = parse_c33("a\tU\textra\n").expect_err("fields");
    assert!(error.to_string().contains("two tab-separated"));
}

#[test]
fn parse_rejects_an_unknown_marker() {
    let error = parse_c33("a\tT\n").expect_err("marker");
    assert!(error.to_string().contains("unknown type"));
}

#[test]
fn parse_rejects_an_empty_name() {
    let error = parse_c33("\tU\n").expect_err("name");
    assert!(error.to_string().contains("empty symbol"));
}

#[test]
fn parse_accepts_both_markers_and_blank_lines() {
    let parsed = parse_c33("a\tU\n\nb\tw\n").expect("parse");
    assert_eq!(parsed, vec![("a".to_string(), 'U'), ("b".to_string(), 'w')]);
}

#[test]
fn the_command_names_both_inputs() {
    let command = census_command(Path::new("/r/x86_64/undef"), Path::new("/res"));
    assert!(command.contains("libc-census"));
    assert!(command.contains("--undef /r/x86_64/undef"));
    assert!(command.contains("--resources /res"));
}

#[test]
fn the_written_file_has_the_c33_columns_exactly() {
    let temp = TempDir::new("census-columns");
    let mut map = BTreeMap::new();
    map.insert("strlcpy".to_string(), 'U');
    let path = write_undef(temp.path(), "x86_64", "pkg", &map).expect("write");
    let bytes = fs::read(&path).expect("read");
    assert_eq!(bytes, b"strlcpy\tU\n");
}
