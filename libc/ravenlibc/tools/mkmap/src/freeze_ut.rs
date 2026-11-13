// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `freeze`.

use super::*;
use crate::testutil;
use crate::tools::DynSymbol;

fn symbol(name: &str, version: &str, kind: &str, size: u64) -> DynSymbol {
    DynSymbol {
        name: name.to_string(),
        version: version.to_string(),
        kind: kind.to_string(),
        size,
    }
}

#[test]
fn renders_sorted_lines_with_kind_and_size() {
    let rendered = render(
        &[
            symbol("z", "RAVEN_1.0", "OBJECT", 8),
            symbol("a", "RAVEN_1.0", "FUNC", 16),
        ],
        "RAVEN_1.0",
        "x86_64",
    );
    let lines = rendered
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(lines, vec!["a F 16 x86_64", "z D 8 x86_64"]);
}

#[test]
fn filters_by_node() {
    let rendered = render(
        &[
            symbol("a", "RAVEN_1.0", "FUNC", 4),
            symbol("b", "RAVENprivate_1.0", "FUNC", 4),
        ],
        "RAVEN_1.0",
        "x86_64",
    );
    assert!(rendered.contains("a F 4"));
    assert!(!rendered.contains("b F 4"));
}

#[test]
fn refuses_to_overwrite_a_different_frozen_file() {
    let dir = testutil::tempdir("freeze-refuse");
    let out = dir.path.join("frozen");
    testutil::write(&dir.path, "frozen/RAVEN_1.0.txt", "old\n");
    let error = write(&out, "RAVEN_1.0", "# new\n").expect_err("refuse");
    assert!(error.to_string().contains("never rewritten"));
}

#[test]
fn writing_the_same_file_twice_is_idempotent() {
    let dir = testutil::tempdir("freeze-idempotent");
    let out = dir.path.join("frozen");
    write(&out, "RAVEN_1.0", "# same\n").expect("first");
    write(&out, "RAVEN_1.0", "# same\n").expect("second");
    let text = std::fs::read_to_string(out.join("RAVEN_1.0.txt")).expect("read");
    assert_eq!(text, "# same\n");
}

#[test]
fn rejects_an_unsafe_node_name() {
    let dir = testutil::tempdir("freeze-node");
    assert!(write(&dir.path, "../escape", "# x\n").is_err());
}
