// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;

use super::*;
use crate::testutil::{TempDir, require, write};

fn header(reason: &str, class: &str) -> String {
    format!("Reason: {reason}\nClass: {class}\n--- a/x\n+++ b/x\n")
}

#[test]
fn a_valid_header_parses() {
    let parsed = parse_header(&header("portable build", "platform")).expect("parse");
    assert_eq!(parsed.reason, "portable build");
    assert_eq!(parsed.class, PatchClass::Platform);
}

#[test]
fn the_reason_is_trimmed() {
    let parsed = parse_header("Reason:   spaced   \nClass: libc\n").expect("parse");
    assert_eq!(parsed.reason, "spaced");
}

#[test]
fn a_missing_reason_line_is_rejected() {
    let error = parse_header("Class: libc\n").expect_err("line 1");
    assert!(error.to_string().contains("line 1"));
}

#[test]
fn an_empty_reason_is_rejected() {
    let error = parse_header("Reason:\nClass: libc\n").expect_err("empty");
    assert!(error.to_string().contains("empty reason"));
}

#[test]
fn a_missing_class_line_is_rejected() {
    let error = parse_header("Reason: x\n").expect_err("line 2");
    assert!(error.to_string().contains("line 2"));
}

#[test]
fn a_bad_class_word_is_rejected() {
    let error = parse_header("Reason: x\nClass: vendor\n").expect_err("class");
    assert!(error.to_string().contains("unknown class"));
}

#[test]
fn a_libc_class_patch_is_a_libc_patch() {
    let patch = classify(
        "a.patch",
        parse_header(&header("add missing function", "libc")).expect("h"),
    );
    assert_eq!(account(&[patch]).libc, 1);
}

#[test]
fn a_platform_patch_without_keywords_is_platform() {
    let patch = classify(
        "a.patch",
        parse_header(&header("config.sub triple", "platform")).expect("h"),
    );
    let counts = account(&[patch]);
    assert_eq!(counts.platform, 1);
    assert_eq!(counts.libc, 0);
}

#[test]
fn a_toolchain_patch_is_counted_separately() {
    let patch = classify(
        "a.patch",
        parse_header(&header("crtbegin", "toolchain")).expect("h"),
    );
    let counts = account(&[patch]);
    assert_eq!(counts.toolchain, 1);
    assert_eq!(counts.platform, 0);
    assert_eq!(counts.libc, 0);
}

#[test]
fn every_libc_keyword_reclassifies_a_platform_patch() {
    let keywords = [
        "missing function",
        "missing header",
        "missing macro",
        "errno",
        "struct layout",
        "sysconf",
        "locale",
        "iconv",
        "symbol version",
        "undefined reference",
        "implicit declaration",
    ];
    for keyword in keywords {
        let reason = format!("fix {keyword} handling");
        assert!(reason_names_libc(&reason), "{keyword}");
        let patch = classify(
            "a.patch",
            parse_header(&header(&reason, "platform")).expect("h"),
        );
        assert!(patch.reclassified_as_libc, "{keyword}");
        assert_eq!(account(&[patch]).libc, 1, "{keyword}");
    }
}

#[test]
fn keyword_matching_is_case_insensitive() {
    assert!(reason_names_libc("Missing Header for foo"));
    assert!(reason_names_libc("ERRNO value differs"));
}

#[test]
fn a_libc_class_patch_is_not_marked_reclassified() {
    let patch = classify(
        "a.patch",
        parse_header(&header("missing header", "libc")).expect("h"),
    );
    assert!(!patch.reclassified_as_libc);
}

#[test]
fn patch_files_are_byte_sorted() {
    let temp = TempDir::new("patches-order");
    for name in ["10-b.patch", "2-a.patch", "01-c.patch"] {
        write(&temp.join(name), &header("x", "libc"));
    }
    write(&temp.join("ignored.txt"), "x");
    let files: Vec<String> = list(temp.path())
        .expect("list")
        .iter()
        .map(|path| {
            path.file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(files, vec!["01-c.patch", "10-b.patch", "2-a.patch"]);
}

#[test]
fn a_missing_patch_directory_is_empty() {
    let temp = TempDir::new("patches-missing");
    assert!(list(&temp.join("nope")).expect("list").is_empty());
}

#[test]
fn a_patch_without_a_header_fails_the_package() {
    if !require(&["patch"]) {
        return;
    }
    let temp = TempDir::new("patch-bad");
    let src = temp.join("src");
    fs::create_dir_all(&src).expect("src");
    write(&src.join("a.txt"), "old\n");
    let patches = temp.join("patches");
    write(&patches.join("bad.patch"), "--- a/a.txt\n+++ b/a.txt\n");
    let error = apply_all("patch", &patches, &src).expect_err("header");
    assert!(error.to_string().contains("patch-header"));
}

#[test]
fn patches_apply_in_order() {
    if !require(&["patch"]) {
        return;
    }
    let temp = TempDir::new("patch-apply");
    let src = temp.join("src");
    fs::create_dir_all(&src).expect("src");
    write(&src.join("a.txt"), "one\n");
    write(&src.join("b.txt"), "one\n");
    let patches = temp.join("patches");
    write(
        &patches.join("01-a.patch"),
        &format!(
            "{}--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-one\n+two\n",
            header("first", "libc")
        ),
    );
    write(
        &patches.join("02-b.patch"),
        &format!(
            "{}--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-one\n+three\n",
            header("second", "platform")
        ),
    );
    let applied = apply_all("patch", &patches, &src).expect("apply");
    assert_eq!(applied.len(), 2);
    assert_eq!(applied[0].file, "01-a.patch");
    assert_eq!(applied[1].file, "02-b.patch");
    assert_eq!(fs::read_to_string(src.join("a.txt")).expect("a"), "two\n");
    assert_eq!(fs::read_to_string(src.join("b.txt")).expect("b"), "three\n");
}

#[test]
fn apply_all_counts_classes() {
    if !require(&["patch"]) {
        return;
    }
    let temp = TempDir::new("patch-count");
    let src = temp.join("src");
    fs::create_dir_all(&src).expect("src");
    write(&src.join("a.txt"), "one\n");
    let patches = temp.join("patches");
    write(
        &patches.join("a.patch"),
        &format!(
            "{}--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-one\n+two\n",
            header("missing header", "platform")
        ),
    );
    let applied = apply_all("patch", &patches, &src).expect("apply");
    let counts = account(&applied);
    assert_eq!(counts.libc, 1);
    assert_eq!(counts.platform, 0);
    assert!(applied[0].reclassified_as_libc);
}
