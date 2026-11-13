// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Regenerate the committed table and compare every byte.

#[path = "support/gen.rs"]
mod ucdgen;

use std::path::{Path, PathBuf};

fn ucd() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../xtask/data/ucd-15.1.0")
}

#[test]
fn regenerated_table_is_byte_identical() {
    let first = ucdgen::generate(&ucd()).expect("generator run 1");
    let committed = include_str!("../src/unicode/tables.rs");
    assert!(first.text == committed, "committed tables.rs is stale");
    let second = ucdgen::generate(&ucd()).expect("generator run 2");
    assert!(first.text == second.text, "generator is not deterministic");
    assert_eq!(first.table_bytes, 33_304);
    assert_eq!(first.props_blocks, 157);
    assert_eq!(first.props_stage2_bytes, 20_096);
    assert_eq!(first.case_blocks, 27);
    assert_eq!(first.case_pairs, 179);
    assert_eq!(first.hashes.len(), 6);
}

#[test]
fn input_headers_say_unicode_15_1_0() {
    let committed = include_str!("../src/unicode/tables.rs");
    assert!(committed.contains("DerivedCoreProperties-15.1.0.txt"));
    assert!(committed.contains("EastAsianWidth-15.1.0.txt"));
    assert!(committed.contains("PropList-15.1.0.txt"));
    assert!(committed.contains("UNICODE, INC. LICENSE AGREEMENT"));
}
