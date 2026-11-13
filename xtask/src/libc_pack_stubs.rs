// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The stub-library hand-over (C66). The body is a stub until `libc/ravenlibc/stubs/Cargo.toml`
//! exists; the generated empty archives and `INPUT(libc.so.1)` scripts are C66's step.

use std::path::Path;

use anyhow::Result;

/// Run the step.
pub(crate) fn run(root: &Path, _out: &Path) -> Result<String> {
    if root.join("libc/ravenlibc/stubs/Cargo.toml").is_file() {
        Ok("ok".to_string())
    } else {
        Ok("skipped(no libc/ravenlibc/stubs)".to_string())
    }
}
