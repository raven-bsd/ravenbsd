// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tarball listing, safety checks, checksum and unpack (section 4.3 step 2).

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::sha256::{Sha256, hex};

/// Entry kind read from `tar -tvf`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Hard link to another archive entry.
    Hardlink,
    /// Any other type (device, fifo, socket, ...).
    Other(char),
}

/// One archive entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// Stored path.
    pub name: String,
    /// Entry kind.
    pub kind: EntryKind,
    /// Link target for symlinks and hard links.
    pub target: Option<String>,
}

/// Why an archive is refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TarProblem {
    /// Traversal, absolute path, escaping link or a special file.
    Unsafe(String),
    /// Not a single-top-directory archive.
    Layout(String),
}

impl TarProblem {
    /// The result status token.
    #[must_use]
    pub fn status(&self) -> &'static str {
        match self {
            Self::Unsafe(_) => "tarball-unsafe",
            Self::Layout(_) => "tarball-layout",
        }
    }

    /// The reason text.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Unsafe(reason) | Self::Layout(reason) => reason,
        }
    }
}

/// List an archive with `tar -tvf`.
pub fn list(tar: &str, tarball: &Path) -> Result<Vec<Entry>> {
    let output = Command::new(tar)
        .arg("-tvf")
        .arg(tarball)
        .output()
        .with_context(|| format!("cannot run `{tar} -tvf {}`", tarball.display()))?;
    if !output.status.success() {
        bail!(
            "`{tar} -tvf {}` failed: {}",
            tarball.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(parse_verbose(line)?);
    }
    Ok(entries)
}

fn parse_verbose(line: &str) -> Result<Entry> {
    let mut chars = line.chars();
    let Some(type_char) = chars.next() else {
        bail!("empty tar listing line");
    };
    let mut rest = chars.as_str();
    // perms, owner/group, size, date, time, then the name (which may contain spaces).
    for _ in 0..5 {
        rest = rest.trim_start();
        let Some(end) = rest.find(char::is_whitespace) else {
            bail!("cannot parse tar listing line `{line}`");
        };
        rest = &rest[end..];
    }
    let name_field = rest.trim_start();
    if name_field.is_empty() {
        bail!("tar listing line has no name `{line}`");
    }
    let (name, target) = match type_char {
        '-' => (name_field.to_string(), None),
        'd' => (name_field.trim_end_matches('/').to_string(), None),
        'l' => {
            let (name, target) = name_field
                .split_once(" -> ")
                .ok_or_else(|| anyhow::anyhow!("symbolic link without target `{line}`"))?;
            (name.to_string(), Some(target.to_string()))
        }
        'h' => {
            let (name, target) = name_field
                .split_once(" link to ")
                .ok_or_else(|| anyhow::anyhow!("hard link without target `{line}`"))?;
            (name.to_string(), Some(target.to_string()))
        }
        other => (name_field.to_string(), Some(other.to_string())),
    };
    let kind = match type_char {
        '-' => EntryKind::File,
        'd' => EntryKind::Directory,
        'l' => EntryKind::Symlink,
        'h' => EntryKind::Hardlink,
        other => EntryKind::Other(other),
    };
    let target = if matches!(kind, EntryKind::Symlink | EntryKind::Hardlink) {
        target
    } else {
        None
    };
    Ok(Entry { name, kind, target })
}

/// Check every safety and layout rule.
pub fn check(entries: &[Entry]) -> std::result::Result<(), TarProblem> {
    if entries.is_empty() {
        return Err(TarProblem::Layout("archive is empty".to_string()));
    }
    let mut roots = std::collections::BTreeSet::new();
    for entry in entries {
        let name = entry.name.replace('\\', "/");
        if name.starts_with('/') {
            return Err(TarProblem::Unsafe(format!(
                "absolute path `{}`",
                entry.name
            )));
        }
        let Some(normalized) = normalize(&name) else {
            return Err(TarProblem::Unsafe(format!(
                "path escapes the root: `{}`",
                entry.name
            )));
        };
        if normalized.is_empty() {
            return Err(TarProblem::Unsafe("empty entry name".to_string()));
        }
        match entry.kind {
            EntryKind::File | EntryKind::Directory => {}
            EntryKind::Other(kind) => {
                return Err(TarProblem::Unsafe(format!(
                    "special entry type `{kind}` for `{}`",
                    entry.name
                )));
            }
            EntryKind::Symlink | EntryKind::Hardlink => {
                let Some(target) = &entry.target else {
                    return Err(TarProblem::Unsafe(format!(
                        "link `{}` has no target",
                        entry.name
                    )));
                };
                if target.starts_with('/') {
                    return Err(TarProblem::Unsafe(format!(
                        "link `{}` points outside the root at `{target}`",
                        entry.name
                    )));
                }
                let parent = normalized.rsplit_once('/').map_or("", |(parent, _)| parent);
                let joined = if parent.is_empty() {
                    target.clone()
                } else {
                    format!("{parent}/{target}")
                };
                if normalize(&joined).is_none() {
                    return Err(TarProblem::Unsafe(format!(
                        "link `{}` escapes the root at `{target}`",
                        entry.name
                    )));
                }
            }
        }
        let root = normalized.split('/').next().unwrap_or_default();
        roots.insert(root.to_string());
    }
    if roots.len() != 1 {
        return Err(TarProblem::Layout(format!(
            "archive has {} top-level entries, expected one",
            roots.len()
        )));
    }
    let root = roots.into_iter().next().unwrap_or_default();
    if !entries
        .iter()
        .any(|entry| normalize(&entry.name).is_some_and(|name| name.contains('/')))
    {
        return Err(TarProblem::Layout(format!(
            "archive has no files under the single top directory `{root}`"
        )));
    }
    Ok(())
}

/// Normalise a relative path; `None` when it escapes the root.
#[must_use]
pub fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            value => parts.push(value),
        }
    }
    Some(parts.join("/"))
}

/// SHA-256 of a file, streamed in 64 KiB chunks.
pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut state = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        state.update(&buffer[..read]);
    }
    Ok(hex(&state.finish()))
}

/// Unpack with `--strip-components=1` into an existing directory.
pub fn extract(tar: &str, tarball: &Path, dest: &Path) -> Result<()> {
    let status = Command::new(tar)
        .arg("-xf")
        .arg(tarball)
        .arg("-C")
        .arg(dest)
        .arg("--strip-components=1")
        .status()
        .with_context(|| format!("cannot run `{tar} -xf {}`", tarball.display()))?;
    if !status.success() {
        bail!("`{tar} -xf {}` failed", tarball.display());
    }
    Ok(())
}

#[cfg(test)]
#[path = "tarball_ut.rs"]
mod tests;
