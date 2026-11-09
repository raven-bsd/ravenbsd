// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the integration tests.
//!
//! Some architecture-specific data (aarch64 and riscv64 layouts and flags) cannot be
//! instantiated on an x86-64 host through `linux-raw-sys`; those tests read the same
//! `linux-raw-sys` source files the table generator reads.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the `linux-raw-sys` package selected by Cargo.
pub fn linux_raw_sys_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cannot run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = String::from_utf8(output.stdout).expect("cargo metadata output is not UTF-8");

    let needle = "\"name\":\"linux-raw-sys\",\"version\":\"";
    let start = metadata
        .find(needle)
        .expect("linux-raw-sys missing from metadata")
        + needle.len();
    let end = metadata[start..]
        .find('"')
        .expect("unterminated linux-raw-sys version");
    assert!(
        metadata[start..start + end].starts_with("0.12."),
        "unexpected linux-raw-sys version {}",
        &metadata[start..start + end]
    );

    let key = "\"manifest_path\":\"";
    let after_name = &metadata[start + end..];
    let manifest_start = after_name
        .find(key)
        .expect("linux-raw-sys has no manifest_path")
        + key.len();
    let manifest_end = after_name[manifest_start..]
        .find('"')
        .expect("unterminated linux-raw-sys manifest_path");
    PathBuf::from(&after_name[manifest_start..manifest_start + manifest_end])
        .parent()
        .expect("linux-raw-sys manifest has no parent")
        .to_path_buf()
}

/// Parse `pub const NAME: u32 = N;` lines with `prefix` from a `linux-raw-sys` source file.
pub fn u32_consts(relative: &str, prefix: &str) -> BTreeMap<String, u32> {
    let path = linux_raw_sys_root().join(relative);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut constants = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(": u32 = ") else {
            continue;
        };
        if !name.starts_with(prefix) {
            continue;
        }
        let Some(value) = tail.strip_suffix(';') else {
            continue;
        };
        constants.insert(
            name.to_owned(),
            value
                .parse()
                .unwrap_or_else(|error| panic!("bad value for {name}: {error}")),
        );
    }
    assert!(!constants.is_empty(), "no {prefix} constants in {relative}");
    constants
}

/// Parse the `__NR_*` table from a `linux-raw-sys` `general.rs` file.
pub fn syscalls(relative: &str) -> BTreeMap<String, u32> {
    let path = linux_raw_sys_root().join(relative);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut syscalls = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub const __NR_") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(": u32 = ") else {
            continue;
        };
        let Some(value) = tail.strip_suffix(';') else {
            continue;
        };
        if name == "syscalls" {
            continue;
        }
        syscalls.insert(
            name.to_owned(),
            value
                .parse()
                .unwrap_or_else(|error| panic!("bad number for __NR_{name}: {error}")),
        );
    }
    assert!(!syscalls.is_empty(), "no syscalls in {relative}");
    syscalls
}
