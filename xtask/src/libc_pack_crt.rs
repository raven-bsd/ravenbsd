// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `crt` hand-over (C61). The body is a stub until `libc/ravenlibc/crt/Cargo.toml` exists.

use std::path::Path;

use anyhow::Result;

/// Run the step; prints the skip reason while the crt crate is absent.
pub(crate) fn run(root: &Path, _out: &Path) -> Result<String> {
    if root.join("libc/ravenlibc/crt/Cargo.toml").is_file() {
        Ok("ok".to_string())
    } else {
        Ok("skipped(no libc/ravenlibc/crt)".to_string())
    }
}
