// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host end-to-end checks for the bounded patch core.

use std::collections::BTreeMap;

use raven_patch::{FileSystem, Options, PatchError, apply};

#[derive(Default)]
struct Mem {
    files: BTreeMap<String, String>,
    writes: usize,
}

impl FileSystem for Mem {
    fn read(&mut self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn write(&mut self, path: &str, contents: &str) -> bool {
        self.writes = self.writes.saturating_add(1);
        self.files.insert(path.to_string(), contents.to_string());
        true
    }

    fn remove(&mut self, path: &str) -> bool {
        self.files.remove(path).is_some()
    }
}

#[test]
fn first_party_diff_applies() {
    let original = "alpha\nbeta\n";
    let modified = "alpha\nBETA\ngamma\n";
    let (changed, patch) = raven_diff::diff(original, modified, true);
    assert!(changed);
    let mut files = Mem::default();
    files.files.insert("b".to_string(), original.to_string());
    let summary = apply(&patch, &Options::default(), &mut files).unwrap();
    assert_eq!(files.files.get("b").map(String::as_str), Some(modified));
    assert_eq!(summary.hunks, 1);
}

#[test]
fn first_party_diff_reverses() {
    let original = "alpha\nbeta\n";
    let modified = "alpha\nBETA\ngamma\n";
    let (_, patch) = raven_diff::diff(original, modified, true);
    let mut files = Mem::default();
    files.files.insert("a".to_string(), modified.to_string());
    let options = Options {
        reverse: true,
        ..Options::default()
    };
    apply(&patch, &options, &mut files).unwrap();
    assert_eq!(files.files.get("a").map(String::as_str), Some(original));
}

#[test]
fn first_party_diff_creates_a_file() {
    let modified = "one\ntwo\n";
    let (changed, patch) = raven_diff::diff("", modified, true);
    assert!(changed);
    let mut files = Mem::default();
    let summary = apply(&patch, &Options::default(), &mut files).unwrap();
    assert_eq!(files.files.get("b").map(String::as_str), Some(modified));
    assert_eq!(summary.created, 1);
}

#[test]
fn first_party_diff_rejects_a_bad_target() {
    let (_, patch) = raven_diff::diff("alpha\nbeta\n", "alpha\nBETA\n", true);
    let mut files = Mem::default();
    files.files.insert("b".to_string(), "wrong\n".to_string());
    assert_eq!(
        apply(&patch, &Options::default(), &mut files),
        Err(PatchError::HunkMismatch {
            file: "b".to_string(),
            hunk: 1,
        })
    );
    assert_eq!(files.writes, 0);
}
