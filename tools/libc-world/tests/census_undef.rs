// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Proves that the `.undef` output is the exact C33 `--undef` input.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libc_world::census_out::write_undef;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn resources(root: &Path) -> Option<PathBuf> {
    let mut directory = Some(root);
    while let Some(current) = directory {
        let candidate = current.join("ravenbsd-resources");
        if candidate.is_dir() {
            return Some(candidate);
        }
        directory = current.parent();
    }
    None
}

#[test]
fn the_undef_output_is_consumed_by_libc_census() {
    let root = repo_root();
    if !root.join("tools/libc-census/Cargo.toml").is_file() {
        eprintln!("SKIP: tools/libc-census absent");
        return;
    }
    let Some(resources) = resources(&root) else {
        eprintln!("SKIP: ravenbsd-resources absent");
        return;
    };
    let temp = std::env::temp_dir().join(format!("libc-world-census-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp");
    let results = temp.join("results");
    let mut symbols = BTreeMap::new();
    symbols.insert("strlcpy".to_string(), 'U');
    symbols.insert("weakthing".to_string(), 'w');
    write_undef(&results, "x86_64", "zlib", &symbols).expect("write");
    let out = temp.join("out");
    let status = Command::new("cargo")
        .current_dir(&root)
        .args([
            "run",
            "-q",
            "-p",
            "libc-census",
            "--",
            "run",
            "--donors",
            "openbsd",
        ])
        .arg("--undef")
        .arg(results.join("x86_64/undef"))
        .arg("--resources")
        .arg(&resources)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("cargo");
    assert!(status.success(), "libc-census run failed");
    let uses = fs::read_to_string(out.join("uses.json")).expect("uses.json");
    assert!(uses.contains("\"strlcpy\": 1"), "{uses}");
    assert!(uses.contains("\"weakthing\": 1"), "{uses}");
    let _ = fs::remove_dir_all(&temp);
}
