// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the temporary-directory helper.

use super::*;

#[test]
fn tempdir_creates_reads_and_removes() {
    let temp = TempDir::new("self");
    temp.write("a/b.txt", "hello");
    assert!(temp.exists("a/b.txt"));
    assert_eq!(temp.read("a/b.txt"), "hello");
    temp.write_bytes("raw.bin", &[0, 1, 2]);
    let path = temp.path().to_path_buf();
    drop(temp);
    assert!(!path.exists());
}

#[test]
fn fixture_path_points_at_the_crate() {
    let path = fixture("table15-excerpt.md");
    assert!(path.starts_with(env!("CARGO_MANIFEST_DIR")));
    assert!(path.ends_with("tests/fixtures/table15-excerpt.md"));
}
