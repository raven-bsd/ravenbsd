// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Licence re-check for donor sources (ticket C35, section 4.4.2; guide 6.5).
//!
//! This is a safety net, not the human gate of guide 1.3: every reject phrase refuses the file,
//! and a file whose first 60 lines contain no copyright or permission text refuses unless it is a
//! musl file and the donor-level `COPYRIGHT` exists (H-1).

use std::path::Path;

use anyhow::{Context, Result};

use crate::failure::Failure;

/// Phrases that refuse a donor file wherever they occur.
pub(crate) const REJECT_PHRASES: &[&str] = &[
    "All advertising materials mentioning features",
    "GNU General Public",
    "GNU Lesser General Public",
    "Common Development and Distribution",
    "BEER-WARE",
];

/// Check one donor file; a refusal is exit 1.
pub(crate) fn check_file(path: &Path, donor: &str, resources: &Path) -> Result<()> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("cannot read donor file {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    for phrase in REJECT_PHRASES {
        if text.contains(phrase) {
            return Err(Failure::refused(format!(
                "licence gate: {} contains `{phrase}`",
                path.display()
            )));
        }
    }
    let first_lines: String = text.lines().take(60).collect::<Vec<_>>().join("\n");
    let has_notice = ["Copyright", "Permission", "Public domain", "public domain"]
        .iter()
        .any(|phrase| first_lines.contains(phrase));
    if has_notice {
        return Ok(());
    }
    if donor == "musl" {
        let copyright = resources.join("musl").join("COPYRIGHT");
        if copyright.is_file() {
            return Ok(());
        }
    }
    Err(Failure::refused(format!(
        "licence gate: {} has no copyright or permission text in its first 60 lines",
        path.display()
    )))
}

#[cfg(test)]
#[path = "gate_ut.rs"]
mod tests;
