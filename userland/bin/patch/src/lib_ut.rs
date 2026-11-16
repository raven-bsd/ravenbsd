// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host unit tests for the bounded patch core.

use std::collections::BTreeMap;

use super::*;

#[derive(Default)]
struct Mem {
    files: BTreeMap<String, String>,
    writes: usize,
    removes: usize,
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
        self.removes = self.removes.saturating_add(1);
        self.files.remove(path).is_some()
    }
}

fn mem(files: &[(&str, &str)]) -> Mem {
    let mut memory = Mem::default();
    for (path, contents) in files {
        memory
            .files
            .insert((*path).to_string(), (*contents).to_string());
    }
    memory
}

#[test]
fn applies_a_unified_change() {
    let patch = "--- a\n+++ b\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
    let mut files = mem(&[("b", "one\ntwo\nthree\n")]);
    let summary = apply(patch, &Options::default(), &mut files).unwrap();
    assert_eq!(
        files.files.get("b").map(String::as_str),
        Some("one\nTWO\nthree\n")
    );
    assert_eq!(summary.files, 1);
    assert_eq!(summary.hunks, 1);
}

#[test]
fn reverses_a_change() {
    let patch = "--- a\n+++ b\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
    let mut files = mem(&[("a", "one\nTWO\nthree\n")]);
    let options = Options {
        reverse: true,
        ..Options::default()
    };
    apply(patch, &options, &mut files).unwrap();
    assert_eq!(
        files.files.get("a").map(String::as_str),
        Some("one\ntwo\nthree\n")
    );
}

#[test]
fn applies_multiple_hunks_with_offsets() {
    let patch = concat!(
        "--- a\n+++ b\n",
        "@@ -1,2 +1,3 @@\n one\n+inserted\n two\n",
        "@@ -4,2 +5,2 @@\n four\n-five\n+FIVE\n"
    );
    let mut files = mem(&[("b", "one\ntwo\nthree\nfour\nfive\n")]);
    apply(patch, &Options::default(), &mut files).unwrap();
    assert_eq!(
        files.files.get("b").map(String::as_str),
        Some("one\ninserted\ntwo\nthree\nfour\nFIVE\n")
    );
}

#[test]
fn creates_and_deletes_files() {
    let create = concat!(
        "--- /dev/null\n+++ created\n",
        "@@ -0,0 +1,2 @@\n+one\n+two\n"
    );
    let mut files = mem(&[]);
    let summary = apply(create, &Options::default(), &mut files).unwrap();
    assert_eq!(
        files.files.get("created").map(String::as_str),
        Some("one\ntwo\n")
    );
    assert_eq!(summary.created, 1);

    let delete = concat!(
        "--- doomed\n+++ /dev/null\n",
        "@@ -1,2 +0,0 @@\n-one\n-two\n"
    );
    let mut files = mem(&[("doomed", "one\ntwo\n")]);
    let summary = apply(delete, &Options::default(), &mut files).unwrap();
    assert!(!files.files.contains_key("doomed"));
    assert_eq!(summary.deleted, 1);
}

#[test]
fn strips_path_components() {
    let patch = concat!(
        "--- old/dir/file\n+++ new/dir/file\n",
        "@@ -1 +1 @@\n-old\n+new\n"
    );
    let mut files = mem(&[("dir/file", "old\n")]);
    let options = Options {
        strip: 1,
        ..Options::default()
    };
    apply(patch, &options, &mut files).unwrap();
    assert_eq!(
        files.files.get("dir/file").map(String::as_str),
        Some("new\n")
    );
}

#[test]
fn mismatched_hunk_is_rejected_atomically() {
    let patch = concat!(
        "--- a\n+++ b\n",
        "@@ -1 +1 @@\n-one\n+ONE\n",
        "@@ -3 +3 @@\n-three\n+THREE\n"
    );
    let mut files = mem(&[("b", "one\ntwo\nfour\n")]);
    assert_eq!(
        apply(patch, &Options::default(), &mut files),
        Err(PatchError::HunkMismatch {
            file: "b".to_string(),
            hunk: 2,
        })
    );
    assert_eq!(
        files.files.get("b").map(String::as_str),
        Some("one\ntwo\nfour\n")
    );
    assert_eq!(files.writes, 0);
}

#[test]
fn missing_target_is_an_error() {
    let patch = "--- a\n+++ missing\n@@ -1 +1 @@\n-old\n+new\n";
    let mut files = mem(&[]);
    assert_eq!(
        apply(patch, &Options::default(), &mut files),
        Err(PatchError::MissingFile("missing".to_string()))
    );
}

#[test]
fn malformed_patch_is_rejected() {
    let mut files = mem(&[]);
    assert_eq!(
        apply("not a patch\n", &Options::default(), &mut files),
        Err(PatchError::InvalidPatch)
    );
    assert_eq!(
        apply("--- a\n+++ b\n", &Options::default(), &mut files),
        Err(PatchError::InvalidPatch)
    );
}

#[test]
fn multiple_files_in_one_patch() {
    let patch = concat!(
        "--- one\n+++ one\n@@ -1 +1 @@\n-a\n+A\n",
        "--- two\n+++ two\n@@ -1 +1 @@\n-b\n+B\n"
    );
    let mut files = mem(&[("one", "a\n"), ("two", "b\n")]);
    let summary = apply(patch, &Options::default(), &mut files).unwrap();
    assert_eq!(files.files.get("one").map(String::as_str), Some("A\n"));
    assert_eq!(files.files.get("two").map(String::as_str), Some("B\n"));
    assert_eq!(summary.files, 2);
}

#[test]
fn no_newline_markers_are_preserved() {
    let patch = concat!(
        "--- a\n+++ b\n@@ -1 +1 @@\n-old\n",
        "\\ No newline at end of file\n",
        "+new\n",
        "\\ No newline at end of file\n"
    );
    let mut files = mem(&[("b", "old")]);
    apply(patch, &Options::default(), &mut files).unwrap();
    assert_eq!(files.files.get("b").map(String::as_str), Some("new"));
}

#[test]
fn parses_options() {
    let invocation = parse_args(["-R", "-p1", "-i", "p.diff"]).unwrap();
    assert!(invocation.options.reverse);
    assert_eq!(invocation.options.strip, 1);
    assert_eq!(invocation.input, Some("p.diff".to_string()));
    assert!(invocation.patch_files.is_empty());

    let invocation = parse_args(["-p", "2", "-f", "-u", "a.patch", "b.patch"]).unwrap();
    assert_eq!(invocation.options.strip, 2);
    assert!(invocation.options.force);
    assert!(invocation.options.unified);
    assert_eq!(invocation.patch_files, vec!["a.patch", "b.patch"]);

    assert_eq!(parse_args(["-Q"]), Err(PatchError::Usage));
    assert_eq!(parse_args(["-p"]), Err(PatchError::Usage));
    assert_eq!(parse_args(["-p", "x"]), Err(PatchError::Usage));
    assert_eq!(parse_args(["-i", "x", "y"]), Err(PatchError::Usage));
}
