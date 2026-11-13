// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the crate root helpers.

use super::*;

#[test]
fn repo_root_contains_the_manifest() {
    assert!(repo_root().join("Cargo.toml").is_file());
    assert!(repo_root().join("libc/ravenlibc/include/stdio.h").is_file());
}

#[test]
fn include_dir_is_shared() {
    assert!(include_dir().join("sys/cdefs.h").is_file());
}

#[test]
fn install_name_strips_both_trees() {
    let shared = repo_root().join("libc/ravenlibc/include/sys/types.h");
    assert_eq!(install_name(&shared), "sys/types.h");
    let arch = repo_root().join("libc/ravenlibc/arch/aarch64/include/bits/alltypes.h");
    assert_eq!(install_name(&arch), "bits/alltypes.h");
}

#[test]
fn read_returns_header_text() {
    let text = read("libc/ravenlibc/include/stdint.h").expect("read");
    assert!(text.contains("SPDX-License-Identifier: MIT"));
}
