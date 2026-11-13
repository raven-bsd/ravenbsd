// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `manifest.toml`: the single inventory of every installed header.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::repo_root;
use crate::toml_lite;

/// Ownership state of a header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The file exists and this ticket owns it.
    Owned,
    /// The file is expected from a later ticket; it may or may not exist.
    Pending,
    /// The header is deliberately not installed.
    Absent,
}

impl Status {
    /// Parse the manifest spelling.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "owned" => Ok(Self::Owned),
            "pending" => Ok(Self::Pending),
            "absent" => Ok(Self::Absent),
            other => Err(format!("unknown header status `{other}`")),
        }
    }

    /// Manifest spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Pending => "pending",
            Self::Absent => "absent",
        }
    }
}

/// One `[[header]]` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderRow {
    /// Install-relative path, for example `stdio.h` or `bits/stat.h`.
    pub path: String,
    /// Owning ticket id, for example `C37`.
    pub owner: String,
    /// Ownership state.
    pub status: Status,
    /// Whether T1 compiles the header on its own.
    pub standalone: bool,
    /// Ring 1, 2 or 3.
    pub ring: u8,
}

impl HeaderRow {
    /// True when the row belongs to one of the listed ticket ids.
    #[must_use]
    pub fn owned_by(&self, owners: &[String]) -> bool {
        owners.iter().any(|owner| owner == &self.owner)
    }
}

/// Load and validate `manifest.toml` next to this crate.
pub fn load() -> Result<Vec<HeaderRow>, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse(&text)
}

/// Parse a manifest text.
pub fn parse(text: &str) -> Result<Vec<HeaderRow>, String> {
    let doc = toml_lite::parse(text).map_err(|error| format!("manifest.toml: {error}"))?;
    let mut rows = Vec::new();
    for table in doc.get("header") {
        let ring = table.get_int("ring")?;
        let ring = u8::try_from(ring).map_err(|_| format!("ring {ring} out of range"))?;
        if !(1..=3).contains(&ring) {
            return Err(format!("ring {ring} is not 1, 2 or 3"));
        }
        rows.push(HeaderRow {
            path: table.get_str("path")?.to_string(),
            owner: table.get_str("owner")?.to_string(),
            status: Status::parse(table.get_str("status")?)?,
            standalone: table.get_bool("standalone")?,
            ring,
        });
    }
    let mut seen = BTreeSet::new();
    for row in &rows {
        if !seen.insert(row.path.clone()) {
            return Err(format!("duplicate manifest header `{}`", row.path));
        }
    }
    Ok(rows)
}

/// Find one row by install-relative path.
#[must_use]
pub fn find<'a>(rows: &'a [HeaderRow], path: &str) -> Option<&'a HeaderRow> {
    rows.iter().find(|row| row.path == path)
}

/// Headers provided by clang's resource directory rather than by this tree.
pub const COMPILER_HEADERS: &[&str] = &[
    "stddef.h",
    "stdarg.h",
    "stdbool.h",
    "stdalign.h",
    "stdnoreturn.h",
    "iso646.h",
    "stdatomic.h",
    "stdckdint.h",
    "tgmath.h",
    "varargs.h",
    "unwind.h",
    "float.h",
];

/// True when the header comes from clang, not from the install tree.
#[must_use]
pub fn compiler_header(path: &str) -> bool {
    COMPILER_HEADERS.contains(&path)
}

/// True when the header is provided by this tree right now.
#[must_use]
pub fn installed(path: &str, arch: Option<&str>) -> bool {
    let root = repo_root().join("libc/ravenlibc");
    let shared = root.join("include").join(path);
    if shared.is_file() {
        return true;
    }
    match arch {
        Some(arch) => root
            .join("arch")
            .join(arch)
            .join("include")
            .join(path)
            .is_file(),
        None => ["x86_64", "aarch64", "riscv64"].iter().any(|arch| {
            root.join("arch")
                .join(arch)
                .join("include")
                .join(path)
                .is_file()
        }),
    }
}

/// Every physical `.h` file in the two install trees, as install-relative
/// names. Duplicate names across architectures are collapsed.
pub fn disk_headers() -> Result<Vec<String>, String> {
    let root = repo_root().join("libc/ravenlibc");
    let mut names = BTreeSet::new();
    collect(&root.join("include"), Path::new(""), &mut names)?;
    for arch in ["x86_64", "aarch64", "riscv64"] {
        collect(
            &root.join("arch").join(arch).join("include"),
            Path::new(""),
            &mut names,
        )?;
    }
    Ok(names.into_iter().collect())
}

fn collect(directory: &Path, prefix: &Path, names: &mut BTreeSet<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read directory entry: {error}"))?;
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            collect(&path, &relative, names)?;
        } else if path.extension().is_some_and(|extension| extension == "h") {
            names.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// The standalone headers this tree owns, in manifest order.
pub fn owned_standalone(rows: &[HeaderRow]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.status == Status::Owned && row.standalone)
        .map(|row| row.path.clone())
        .collect()
}

/// Parse `HDRCHECK_REQUIRE_OWNERS` (comma list of ticket ids).
#[must_use]
pub fn required_owners() -> Vec<String> {
    std::env::var("HDRCHECK_REQUIRE_OWNERS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `HDRCHECK_REQUIRE_COMPLETE=1`.
#[must_use]
pub fn require_complete() -> bool {
    std::env::var("HDRCHECK_REQUIRE_COMPLETE").is_ok_and(|value| value == "1")
}

/// Parse `HDRCHECK_JOBS`.
#[must_use]
pub fn jobs() -> usize {
    std::env::var("HDRCHECK_JOBS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(1)
        })
}

/// `HDRCHECK_QUICK=1`.
#[must_use]
pub fn quick() -> bool {
    std::env::var("HDRCHECK_QUICK").is_ok_and(|value| value == "1")
}

/// A human-readable reason why a pending row is not a failure for the
/// requested owner set, or an error when it is.
pub fn pending_allowed(owner: &str, required: &[String], complete: bool) -> bool {
    !complete && !required.iter().any(|item| item == owner)
}

#[cfg(test)]
#[path = "manifest_ut.rs"]
mod tests;
