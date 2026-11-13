// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the merge and the flat list.

use super::*;
use crate::versions::{Node, NodeBody, Symbol};

fn chain() -> Vec<Node> {
    vec![
        Node {
            name: "RAVEN_1.0".to_string(),
            parent: None,
        },
        Node {
            name: "RAVENprivate_1.0".to_string(),
            parent: Some("RAVEN_1.0".to_string()),
        },
    ]
}

fn body(node: &str, names: &[(&str, Option<&[&str]>)]) -> NodeBody {
    NodeBody {
        node: node.to_string(),
        symbols: names
            .iter()
            .map(|(name, arches)| Symbol {
                name: (*name).to_string(),
                arches: arches.map(|list| list.iter().map(|arch| (*arch).to_string()).collect()),
            })
            .collect(),
    }
}

#[test]
fn merges_two_nodes() {
    let merged = merge(
        &chain(),
        &[
            body("RAVEN_1.0", &[("b", None), ("a", None)]),
            body("RAVENprivate_1.0", &[("__rl_x", None)]),
        ],
        "x86_64",
    )
    .expect("merge");
    assert_eq!(merged.public, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(merged.private, vec!["__rl_x".to_string()]);
}

#[test]
fn public_names_are_sorted() {
    let merged = merge(
        &chain(),
        &[body("RAVEN_1.0", &[("z", None), ("m", None), ("a", None)])],
        "x86_64",
    )
    .expect("merge");
    let names = merged.public.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(names, vec!["a", "m", "z"]);
}

#[test]
fn arch_comments_filter_the_other_arch() {
    let merged = merge(
        &chain(),
        &[body(
            "RAVEN_1.0",
            &[("both", None), ("arm", Some(&["aarch64"]))],
        )],
        "x86_64",
    )
    .expect("merge");
    let names = merged.public.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(names, vec!["both"]);
    assert!(!merged.map.contains("arm;"));
}

#[test]
fn the_same_name_in_two_nodes_is_an_error() {
    let error = merge(
        &chain(),
        &[
            body("RAVEN_1.0", &[("x", None)]),
            body("RAVENprivate_1.0", &[("x", None)]),
        ],
        "x86_64",
    )
    .expect_err("twice");
    assert!(error.to_string().contains("both"));
}

#[test]
fn a_private_name_is_not_in_the_public_section() {
    let merged = merge(
        &chain(),
        &[
            body("RAVEN_1.0", &[("pub", None)]),
            body("RAVENprivate_1.0", &[("priv", None)]),
        ],
        "x86_64",
    )
    .expect("merge");
    let public_part = merged
        .list
        .split("\n# private\n")
        .next()
        .unwrap_or("")
        .to_string();
    let public_names = public_part
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect::<Vec<_>>();
    assert!(public_names.contains(&"pub"));
    assert!(!public_names.contains(&"priv"));
    assert!(merged.list.contains("# private\npriv\n"));
}

#[test]
fn local_star_only_in_the_last_node() {
    let merged = merge(&chain(), &[body("RAVEN_1.0", &[("a", None)])], "x86_64").expect("merge");
    assert_eq!(merged.map.matches("local:").count(), 1);
    let last = merged.map.split("RAVENprivate_1.0").nth(1).unwrap_or("");
    assert!(last.contains("local:"));
}

#[test]
fn an_empty_block_is_legal() {
    let merged = merge(&chain(), &[], "x86_64").expect("merge");
    assert!(merged.map.contains("RAVEN_1.0 {\n\tglobal:\n};"));
    assert!(merged.public.is_empty());
}

#[test]
fn output_is_byte_identical_on_two_runs() {
    let bodies = [body("RAVEN_1.0", &[("b", None), ("a", None)])];
    let first = merge(&chain(), &bodies, "x86_64").expect("first");
    let second = merge(&chain(), &bodies, "x86_64").expect("second");
    assert_eq!(first.map, second.map);
    assert_eq!(first.list, second.list);
}

#[test]
fn a_body_for_an_unknown_node_is_an_error() {
    let error =
        merge(&chain(), &[body("RAVEN_2.0", &[("a", None)])], "x86_64").expect_err("unknown");
    assert!(error.to_string().contains("absent from Versions.def"));
}

#[test]
fn the_private_node_must_be_last() {
    let nodes = vec![
        Node {
            name: "RAVENprivate_1.0".to_string(),
            parent: None,
        },
        Node {
            name: "RAVEN_1.0".to_string(),
            parent: Some("RAVENprivate_1.0".to_string()),
        },
    ];
    let error = merge(&nodes, &[], "x86_64").expect_err("last");
    assert!(error.to_string().contains("last"));
}

#[test]
fn a_missing_private_node_is_an_error() {
    let nodes = vec![Node {
        name: "RAVEN_1.0".to_string(),
        parent: None,
    }];
    let error = merge(&nodes, &[], "x86_64").expect_err("private");
    assert!(error.to_string().contains("RAVENprivate"));
}

#[test]
fn the_list_parses_back_into_its_two_sections() {
    let merged = merge(
        &chain(),
        &[
            body("RAVEN_1.0", &[("a", None)]),
            body("RAVENprivate_1.0", &[("p", None)]),
        ],
        "x86_64",
    )
    .expect("merge");
    let (public, private) = parse_list(&merged.list);
    assert_eq!(public, vec!["a".to_string()]);
    assert_eq!(private, vec!["p".to_string()]);
}

#[test]
fn a_node_header_keeps_its_parent() {
    let merged = merge(&chain(), &[], "x86_64").expect("merge");
    assert!(merged.map.contains("} RAVEN_1.0;"));
}
