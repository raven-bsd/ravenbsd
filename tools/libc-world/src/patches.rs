// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Package patches: header parsing, libc reclassification and application (section 4.3 step 3).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Declared patch class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PatchClass {
    /// Caused by a libc gap.
    Libc,
    /// Platform or toolchain teaching.
    Platform,
    /// Toolchain only.
    Toolchain,
}

impl PatchClass {
    /// Parse the `Class:` value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "libc" => Some(Self::Libc),
            "platform" => Some(Self::Platform),
            "toolchain" => Some(Self::Toolchain),
            _ => None,
        }
    }

    /// The wire text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Libc => "libc",
            Self::Platform => "platform",
            Self::Toolchain => "toolchain",
        }
    }
}

/// The two mandatory header lines of a patch file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchHeader {
    /// `Reason:` text.
    pub reason: String,
    /// `Class:` value.
    pub class: PatchClass,
}

/// One patch that was applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedPatch {
    /// File name.
    pub file: String,
    /// Parsed header.
    pub header: PatchHeader,
    /// A `platform` patch whose reason still counts as libc.
    pub reclassified_as_libc: bool,
}

/// Patch counts for `result.json`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PatchCounts {
    /// libc patches.
    pub libc: u32,
    /// platform patches.
    pub platform: u32,
    /// toolchain patches.
    pub toolchain: u32,
}

/// Parse the mandatory two-line header.
pub fn parse_header(text: &str) -> Result<PatchHeader> {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        bail!("patch header: file is empty");
    };
    let Some(reason) = first.strip_prefix("Reason:") else {
        bail!("patch header: line 1 must start with `Reason:`");
    };
    let reason = reason.trim();
    if reason.is_empty() {
        bail!("patch header: empty reason");
    }
    let Some(second) = lines.next() else {
        bail!("patch header: line 2 is missing");
    };
    let Some(class) = second.strip_prefix("Class:") else {
        bail!("patch header: line 2 must start with `Class:`");
    };
    let Some(class) = PatchClass::parse(class) else {
        bail!("patch header: unknown class `{}`", class.trim());
    };
    Ok(PatchHeader {
        reason: reason.to_string(),
        class,
    })
}

/// Whether a reason text contains a libc keyword of the conservative rule.
#[must_use]
pub fn reason_names_libc(reason: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "missing function",
        "missing header",
        "missing macro",
        "errno",
        "struct layout",
        "sysconf",
        "locale",
        "iconv",
        "symbol version",
        "undefined reference",
        "implicit declaration",
    ];
    let lower = reason.to_ascii_lowercase();
    KEYWORDS.iter().any(|keyword| lower.contains(keyword))
}

/// Classify one patch header with the conservative reclassification rule.
#[must_use]
pub fn classify(file: &str, header: PatchHeader) -> AppliedPatch {
    let reclassified_as_libc =
        header.class == PatchClass::Platform && reason_names_libc(&header.reason);
    AppliedPatch {
        file: file.to_string(),
        header,
        reclassified_as_libc,
    }
}

/// Count patches by class.
#[must_use]
pub fn account(patches: &[AppliedPatch]) -> PatchCounts {
    let mut counts = PatchCounts::default();
    for patch in patches {
        match patch.header.class {
            PatchClass::Libc => counts.libc += 1,
            PatchClass::Platform => {
                if patch.reclassified_as_libc {
                    counts.libc += 1;
                } else {
                    counts.platform += 1;
                }
            }
            PatchClass::Toolchain => counts.toolchain += 1,
        }
    }
    counts
}

/// Every `*.patch` in `directory`, byte-sorted by file name.
pub fn list(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !directory.is_dir() {
        return Ok(files);
    }
    for entry in
        fs::read_dir(directory).with_context(|| format!("cannot read {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "patch")
        {
            files.push(path);
        }
    }
    files.sort_by(|left, right| {
        let left = left.file_name().unwrap_or_default();
        let right = right.file_name().unwrap_or_default();
        left.as_encoded_bytes().cmp(right.as_encoded_bytes())
    });
    Ok(files)
}

/// Apply every patch in `directory` to `src`.
pub fn apply_all(patch_tool: &str, directory: &Path, src: &Path) -> Result<Vec<AppliedPatch>> {
    let mut applied = Vec::new();
    for path in list(directory)? {
        let file = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let text = fs::read_to_string(&path)
            .with_context(|| format!("cannot read patch {}", path.display()))?;
        let header = parse_header(&text).with_context(|| format!("patch-header: {file}"))?;
        let status = Command::new(patch_tool)
            .arg("-p1")
            .arg("--no-backup-if-mismatch")
            .arg("-d")
            .arg(src)
            .arg("-i")
            .arg(&path)
            .status()
            .with_context(|| format!("cannot run `{patch_tool}`"))?;
        if !status.success() {
            bail!("patch failed to apply: {file}");
        }
        applied.push(classify(&file, header));
    }
    Ok(applied)
}

#[cfg(test)]
#[path = "patches_ut.rs"]
mod tests;
