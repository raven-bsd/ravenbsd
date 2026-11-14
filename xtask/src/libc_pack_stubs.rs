// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The stub-library hand-over (C66). The body is a stub until `libc/ravenlibc/stubs/Cargo.toml`
//! exists; the generated empty archives and `INPUT(libc.so.1)` scripts are C66's step.

use std::path::Path;

use anyhow::Result;

/// Run the step: generate the 20 stub libraries when the C66 table is present.
pub(crate) fn run(root: &Path, out: &Path) -> Result<String> {
    let table_path = root.join("libc/ravenlibc/stubs/stubs.toml");
    if !table_path.is_file() {
        return Ok("skipped(no libc/ravenlibc/stubs)".to_string());
    }
    let table = crate::libc_stubs::Stubs::load(&table_path)?;
    let count = crate::libc_stubs::generate(&table, &out.join("stubs"))?;
    Ok(format!("ok ({count} files)"))
}
