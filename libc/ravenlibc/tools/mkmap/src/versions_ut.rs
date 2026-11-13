// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `Versions.def` and `Symbol.map` parsing.

use super::*;

#[test]
fn parses_the_two_node_chain() {
    let nodes = parse_versions("RAVEN_1.0 { };\nRAVENprivate_1.0 { } RAVEN_1.0;\n").expect("parse");
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        nodes.first().map(|node| node.name.as_str()),
        Some("RAVEN_1.0")
    );
    assert_eq!(
        nodes.last().and_then(|node| node.parent.clone()),
        Some("RAVEN_1.0".to_string())
    );
}

#[test]
fn tolerates_comments_and_blank_lines() {
    let nodes = parse_versions(
        "# header\n\nRAVEN_1.0 { };\n\n# private\nRAVENprivate_1.0 { } RAVEN_1.0;\n",
    )
    .expect("parse");
    assert_eq!(nodes.len(), 2);
}

#[test]
fn rejects_a_duplicate_node() {
    let error = parse_versions("RAVEN_1.0 { };\nRAVEN_1.0 { };\n").expect_err("duplicate");
    assert!(error.to_string().contains("duplicate"));
}

#[test]
fn rejects_an_unknown_parent() {
    let error = parse_versions("RAVENprivate_1.0 { } RAVEN_9.9;\n").expect_err("parent");
    assert!(error.to_string().contains("unknown"));
}

#[test]
fn rejects_a_node_without_a_brace() {
    let error = parse_versions("RAVEN_1.0;\n").expect_err("brace");
    assert!(error.to_string().contains("expected"));
}

#[test]
fn parses_symbols_arches_and_comments() {
    let bodies = parse_symbol_map(
        "# c\nRAVEN_1.0 {\n global:\n  # --- string ---\n  memchr;\n  strlen; # x86_64\n};\n",
        "Symbol.map",
    )
    .expect("parse");
    assert_eq!(bodies.len(), 1);
    let symbols = &bodies.first().expect("body").symbols;
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols.first().map(|s| s.name.as_str()), Some("memchr"));
    assert_eq!(
        symbols.get(1).and_then(|s| s.arches.clone()),
        Some(vec!["x86_64".to_string()])
    );
}

#[test]
fn rejects_two_nodes_of_the_same_name_in_one_file() {
    let error = parse_symbol_map(
        "RAVEN_1.0 {\n global:\n  a;\n};\nRAVEN_1.0 {\n global:\n  b;\n};\n",
        "m",
    )
    .expect_err("twice");
    assert!(error.to_string().contains("twice"));
}

#[test]
fn rejects_a_duplicate_symbol() {
    let error =
        parse_symbol_map("RAVEN_1.0 {\n global:\n  a;\n  a;\n};\n", "m").expect_err("duplicate");
    assert!(error.to_string().contains("duplicate"));
}

#[test]
fn rejects_a_symbol_without_semicolon() {
    let error = parse_symbol_map("RAVEN_1.0 {\n global:\n  a\n};\n", "m").expect_err("semi");
    assert!(error.to_string().contains("name;"));
}

#[test]
fn strips_only_real_comments() {
    assert_eq!(strip_comment("a; # x").trim(), "a;");
    assert_eq!(strip_comment("# all").trim(), "");
    assert_eq!(strip_comment("a;b"), "a;b");
}

#[test]
fn arch_matching_is_explicit() {
    let symbol = Symbol {
        name: "x".to_string(),
        arches: Some(vec!["aarch64".to_string(), "riscv64".to_string()]),
    };
    assert!(!symbol.on_arch("x86_64"));
    assert!(symbol.on_arch("aarch64"));
    let all = Symbol {
        name: "y".to_string(),
        arches: None,
    };
    assert!(all.on_arch("riscv64"));
}
