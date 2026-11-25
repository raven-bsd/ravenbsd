// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared host-test support: locate the read-only donor checkout without a panic.

use std::path::{Path, PathBuf};

/// Product clone root for the invoking test binary.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// Locate `ravenbsd-resources` from `RAVENBSD_RESOURCES` or at/above the clone root.
///
/// An explicit variable must point at a real checkout; a discovered sibling is optional so a
/// clean scratch clone with no donor checkout skips the donor-dependent rows instead of aborting.
pub fn resources() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("RAVENBSD_RESOURCES") {
        let path = PathBuf::from(value);
        assert!(
            path.join("donors.lock").is_file(),
            "RAVENBSD_RESOURCES has no donors.lock: {}",
            path.display()
        );
        return Some(path);
    }
    let mut directory = Some(repo_root());
    while let Some(current) = directory {
        let candidate = current.join("ravenbsd-resources");
        if candidate.join("donors.lock").is_file() {
            return Some(candidate);
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    None
}

/// Return the donor checkout or print a not-run diagnostic and let the caller skip.
pub fn require_resources(test: &str) -> Option<PathBuf> {
    match resources() {
        Some(path) => Some(path),
        None => {
            eprintln!(
                "SKIP {test}: ravenbsd-resources/donors.lock is absent; \
                 set RAVENBSD_RESOURCES to a donor checkout to run this row"
            );
            None
        }
    }
}
