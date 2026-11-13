// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host-only header test matrix of ravenlibc.
//!
//! This crate is not shipped. It locates the host `clang`, feeds one
//! translation unit per process, and checks the C headers of C37: standalone
//! compilation, include order, layouts, constants, feature macros, compiler
//! header shadowing, hygiene and the manifest. Doc 14 3.B.4 is the contract.

#![forbid(unsafe_code)]

pub mod clang;
pub mod consts;
pub mod layout;
pub mod manifest;
pub mod matrix;
pub mod toml_lite;

use std::path::PathBuf;

pub use clang::{
    Arch, Lang, Outcome, Toolchain, TripleFlavour, Tu, Vis, clang_args, compile, locate_toolchain,
    run_matrix,
};

/// The repository root, independent of the current working directory.
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

/// The shared include directory.
#[must_use]
pub fn include_dir() -> PathBuf {
    repo_root().join("libc/ravenlibc/include")
}

/// The install-relative path of a physical header.
#[must_use]
pub fn install_name(physical: &std::path::Path) -> String {
    let root = repo_root().join("libc/ravenlibc");
    let relative = physical.strip_prefix(&root).unwrap_or(physical);
    let text = relative.to_string_lossy().replace('\\', "/");
    if let Some(rest) = text.strip_prefix("include/") {
        return rest.to_string();
    }
    for arch in ["x86_64", "aarch64", "riscv64"] {
        if let Some(rest) = text.strip_prefix(&format!("arch/{arch}/include/")) {
            return rest.to_string();
        }
    }
    text
}

/// Read a repository-relative file.
pub fn read(relative: &str) -> Result<String, String> {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
