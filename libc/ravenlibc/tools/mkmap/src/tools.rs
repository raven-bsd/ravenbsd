// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! External binary helpers (`nm`, `readelf`) and the parsers of their stable output formats.
//!
//! The pipeline never needs a Rust ELF reader: `nm --format=posix` gives the member/defined/
//! undefined view of an archive and `readelf --dyn-syms -W` the versioned view of a DSO. The
//! programs are overridable through `RL_NM`/`RL_READELF`, which is how the fixture tests run a
//! deterministic stand-in.

use std::fmt;
use std::process::Command;

use anyhow::{Result, bail};

/// A tool named by an environment variable could not be started.
#[derive(Debug)]
pub struct MissingTool(pub String);

impl fmt::Display for MissingTool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} not found", self.0)
    }
}

impl std::error::Error for MissingTool {}

/// One defined symbol of one archive member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveSymbol {
    /// Archive member file name.
    pub member: String,
    /// Symbol name.
    pub name: String,
    /// `nm` type letter (`T`, `D`, `B`, `R`, `W`, `V`, ...).
    pub kind: char,
    /// Size in bytes when the tool printed it.
    pub size: u64,
}

impl ArchiveSymbol {
    /// Text (code) symbol.
    #[must_use]
    pub fn is_text(&self) -> bool {
        matches!(self.kind.to_ascii_uppercase(), 'T' | 'W' | 'V')
    }

    /// Data symbol.
    #[must_use]
    pub fn is_data(&self) -> bool {
        matches!(self.kind.to_ascii_uppercase(), 'D' | 'B' | 'R' | 'G' | 'S')
    }

    /// Weak definition (`W`, `w`, `V`, `v`).
    #[must_use]
    pub fn is_weak(&self) -> bool {
        matches!(self.kind, 'W' | 'w' | 'V' | 'v')
    }
}

/// One defined dynamic symbol of a DSO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynSymbol {
    /// Symbol name without the version suffix.
    pub name: String,
    /// Version string as printed (`RAVEN_1.0`), or empty when unversioned.
    pub version: String,
    /// `readelf` type (`FUNC`, `OBJECT`, ...).
    pub kind: String,
    /// Size in bytes.
    pub size: u64,
}

/// Run one tool and return its standard output.
///
/// # Errors
/// A missing program is [`MissingTool`]; a non-zero exit is an `anyhow` error with stderr.
pub fn run(program: &str, args: &[&str], cwd: &std::path::Path) -> Result<String> {
    let mut attempt = 0usize;
    loop {
        let output = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    anyhow::Error::new(MissingTool(program.to_string()))
                } else {
                    anyhow::Error::new(error).context(format!("cannot run `{program}`"))
                }
            });
        match output {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    bail!("`{program}` failed: {}", stderr.trim());
                }
                return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
            }
            // A just-written test script can still be open for writing for a few milliseconds;
            // retry briefly instead of failing the fixture.
            Err(error)
                if attempt < 20
                    && error.downcast_ref::<std::io::Error>().is_some_and(|inner| {
                        inner.kind() == std::io::ErrorKind::ExecutableFileBusy
                    }) =>
            {
                attempt = attempt.saturating_add(1);
                std::thread::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
}

/// The `nm` program: `RL_NM` or `nm`.
#[must_use]
pub fn nm_program() -> String {
    std::env::var("RL_NM").unwrap_or_else(|_| "nm".to_string())
}

/// The `readelf` program: `RL_READELF` or `readelf`.
#[must_use]
pub fn readelf_program() -> String {
    std::env::var("RL_READELF").unwrap_or_else(|_| "readelf".to_string())
}

/// Parse `nm --format=posix --defined-only --extern-only` output for an archive.
#[must_use]
pub fn parse_nm(output: &str) -> Vec<ArchiveSymbol> {
    let mut member = String::new();
    let mut symbols = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.ends_with(':') {
            member = header_member(line.trim_end_matches(':'));
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind), Some(value), size) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        // Ignore a tool diagnostic that landed on stdout (`nm` prints the bfd-plugin
        // complaint there for a member the host binutils cannot read).
        if kind.len() != 1 || u64::from_str_radix(value, 16).is_err() {
            continue;
        }
        let Some(kind) = kind.chars().next() else {
            continue;
        };
        symbols.push(ArchiveSymbol {
            member: member.clone(),
            name: name.to_string(),
            kind,
            size: size
                .and_then(|text| u64::from_str_radix(text, 16).ok())
                .unwrap_or(0),
        });
    }
    symbols
}

/// Extract the member name from an `nm` header line (`libx.a[m.o]` or `m.o`).
fn header_member(header: &str) -> String {
    if let Some(open) = header.find('[')
        && let Some(close) = header.rfind(']')
        && close > open
    {
        return header.get(open + 1..close).unwrap_or(header).to_string();
    }
    header.to_string()
}

/// Parse `readelf --dyn-syms -W` output.
#[must_use]
pub fn parse_dyn_syms(output: &str) -> Vec<DynSymbol> {
    let mut symbols = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Num:") || line.starts_with("Symbol table") {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 8 {
            continue;
        }
        let Some(kind) = fields.get(3) else { continue };
        if !matches!(*kind, "FUNC" | "OBJECT" | "IFUNC" | "NOTYPE") {
            continue;
        }
        let Some(ndx) = fields.get(6) else { continue };
        if matches!(*ndx, "UND" | "ABS") {
            continue;
        }
        let Some(raw) = fields.get(7) else { continue };
        let (name, version) = match raw.split_once("@@") {
            Some((name, version)) => (name, version),
            None => match raw.split_once('@') {
                Some((name, version)) => (name, version),
                None => (*raw, ""),
            },
        };
        let size = fields
            .get(2)
            .and_then(|text| u64::from_str_radix(text, 16).ok())
            .unwrap_or(0);
        symbols.push(DynSymbol {
            name: name.to_string(),
            version: version.to_string(),
            kind: (*kind).to_string(),
            size,
        });
    }
    symbols
}

/// Count the `JUMP_SLOT` relocation entries of a DSO (`readelf -rW`).
#[must_use]
pub fn count_jump_slots(relocations: &str) -> usize {
    relocations
        .lines()
        .filter(|line| {
            line.split_whitespace()
                .any(|field| field.ends_with("JUMP_SLOT"))
        })
        .count()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "tools_ut.rs"]
mod tests;
