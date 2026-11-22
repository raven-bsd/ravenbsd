// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Product documentation is frozen (AGENTS 31).
//!
//! The former man-page coverage/rendering scan read `docs/man*`, which is a dead product tree.
//! K497 removed that scan and left a no-op with an explicit reason so `cargo xtask check` and
//! `make docs` keep a stable entry point. Documentation and evidence live in
//! `ravenbsd-prjmgmt/`.

use std::path::Path;

use anyhow::Result;

/// Report that product documentation is frozen; never read `docs/`.
pub(crate) fn run(_root: &Path) -> Result<u8> {
    println!(
        "docs: product documentation is frozen (AGENTS 31); the man-page scan was removed by \
         K497; documentation and evidence live in ravenbsd-prjmgmt"
    );
    Ok(0)
}

#[cfg(test)]
#[path = "docs_ut.rs"]
mod tests;
