// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `find` traversal and arguments.

use std::collections::BTreeMap;

use alloc::string::String;
use alloc::vec::Vec;

use raven_tool_core::Errno;

use super::{Kind, Options, Tree, glob, parse, walk};

struct Mock {
    entries: BTreeMap<String, (Kind, u32)>,
    children: BTreeMap<String, Vec<String>>,
}

impl Tree for Mock {
    fn kind(&mut self, path: &str) -> Result<Kind, Errno> {
        self.entries
            .get(path)
            .map(|entry| entry.0)
            .ok_or(Errno::ENOENT)
    }

    fn mode(&mut self, path: &str) -> Result<u32, Errno> {
        self.entries
            .get(path)
            .map(|entry| entry.1)
            .ok_or(Errno::ENOENT)
    }

    fn children(&mut self, path: &str) -> Result<Vec<String>, Errno> {
        self.children.get(path).cloned().ok_or(Errno::ENOENT)
    }
}

fn tree() -> Mock {
    let mut entries = BTreeMap::new();
    entries.insert(".".to_string(), (Kind::Directory, 0o755));
    entries.insert("./a.txt".to_string(), (Kind::File, 0o644));
    entries.insert("./b.txt".to_string(), (Kind::File, 0o600));
    entries.insert("./dir".to_string(), (Kind::Directory, 0o750));
    entries.insert("./dir/c.log".to_string(), (Kind::File, 0o644));
    entries.insert("./link".to_string(), (Kind::Symlink, 0o777));
    let mut children = BTreeMap::new();
    children.insert(
        ".".to_string(),
        vec![
            "a.txt".to_string(),
            "b.txt".to_string(),
            "dir".to_string(),
            "link".to_string(),
        ],
    );
    children.insert("dir".to_string(), vec!["c.log".to_string()]);
    children.insert("./dir".to_string(), vec!["c.log".to_string()]);
    Mock { entries, children }
}

fn defaults() -> Options<'static> {
    Options {
        paths: vec!["."],
        ..Options::default()
    }
}

#[test]
fn parse_paths_and_predicates() {
    let options = parse([".", "-name", "*.txt", "-type", "f", "-perm", "644"]).unwrap();
    assert_eq!(options.paths, vec!["."]);
    assert_eq!(options.name, Some("*.txt"));
    assert_eq!(options.kind, Some(Kind::File));
    assert_eq!(options.perm, Some(0o644));
}

#[test]
fn parse_defaults_to_dot() {
    assert_eq!(parse(std::iter::empty()).unwrap().paths, vec!["."]);
}

#[test]
fn unknown_option_is_rejected() {
    assert!(parse(["-exec"]).is_err());
}

#[test]
fn default_walk_prints_the_tree() {
    let mut tree = tree();
    let mut out = Vec::new();
    walk(&mut tree, &defaults(), &mut out).unwrap();
    assert!(out.contains(&".".to_string()));
    assert!(out.contains(&"./a.txt".to_string()));
    assert!(out.contains(&"./dir/c.log".to_string()));
    assert!(out.contains(&"./link".to_string()));
}

#[test]
fn name_pattern_filters() {
    let mut tree = tree();
    let mut out = Vec::new();
    let options = Options {
        name: Some("*.txt"),
        ..defaults()
    };
    walk(&mut tree, &options, &mut out).unwrap();
    assert_eq!(out, vec!["./a.txt", "./b.txt"]);
}

#[test]
fn type_and_perm_filter() {
    let mut tree = tree();
    let mut out = Vec::new();
    let options = Options {
        kind: Some(Kind::File),
        perm: Some(0o600),
        ..defaults()
    };
    walk(&mut tree, &options, &mut out).unwrap();
    assert_eq!(out, vec!["./b.txt"]);
}

#[test]
fn symlinks_are_not_followed() {
    let mut tree = tree();
    let mut out = Vec::new();
    let options = Options {
        kind: Some(Kind::Symlink),
        ..defaults()
    };
    walk(&mut tree, &options, &mut out).unwrap();
    assert_eq!(out, vec!["./link"]);
}

#[test]
fn missing_start_is_an_error() {
    let mut tree = tree();
    let mut out = Vec::new();
    let options = Options {
        paths: vec!["./missing"],
        ..Options::default()
    };
    assert_eq!(walk(&mut tree, &options, &mut out), Err(Errno::ENOENT));
}

#[test]
fn glob_matches_star_and_question() {
    assert!(glob("*.txt", "a.txt"));
    assert!(glob("a?c", "abc"));
    assert!(!glob("*.txt", "a.log"));
    assert!(!glob("a?c", "ac"));
}
