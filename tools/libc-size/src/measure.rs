// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Output parsers of the measurement protocol (`C69` 4.4, `donor-map/16` 3.A.7.2).
//!
//! Every parser is pure so the fixtures captured from the pinned tools can be replayed in
//! unit tests. `S = text + data`, `B = bss`, `F` is the stripped file size in bytes.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Berkeley `llvm-size` triple.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Berkeley {
    /// `.text` bytes.
    pub text: u64,
    /// `.data` bytes.
    pub data: u64,
    /// `.bss` bytes.
    pub bss: u64,
}

impl Berkeley {
    /// The size metric `S` of the protocol.
    #[must_use]
    pub fn s(&self) -> u64 {
        self.text.saturating_add(self.data)
    }
}

/// One named entry of `llvm-size` (an executable or a member of an archive).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizeEntry {
    /// File or member name.
    pub name: String,
    /// The triple.
    pub size: Berkeley,
}

/// Parse Berkeley `llvm-size` output (one or several files).
///
/// Lines before the header are ignored; the header line is the one containing `text` and
/// `filename`. Every following numeric line is an entry.
///
/// # Errors
/// A malformed numeric field.
pub fn parse_berkeley(text: &str) -> Result<Vec<SizeEntry>> {
    let mut entries = Vec::new();
    let mut after_header = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains("text") && trimmed.contains("filename") {
            after_header = true;
            continue;
        }
        if !after_header {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let text_bytes = parse_u64(fields[0])?;
        let data = parse_u64(fields[1])?;
        let bss = parse_u64(fields[2])?;
        let name = fields.get(5..).unwrap_or(&[]).join(" ");
        entries.push(SizeEntry {
            name,
            size: Berkeley {
                text: text_bytes,
                data,
                bss,
            },
        });
    }
    if entries.is_empty() {
        bail!("llvm-size output has no size line");
    }
    Ok(entries)
}

/// One section from `llvm-size -A`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Section name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
}

/// Parse `llvm-size -A` output (`name size addr`).
///
/// # Errors
/// A malformed line.
pub fn parse_sections(text: &str) -> Result<Vec<Section>> {
    let mut sections = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("section") {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 2
            || !fields[0].starts_with('.')
            || fields[0].contains('/')
            || fields[0].contains(':')
        {
            continue;
        }
        let size = parse_u64(fields[1]).with_context(|| format!("section line `{trimmed}`"))?;
        sections.push(Section {
            name: fields[0].to_string(),
            size,
        });
    }
    Ok(sections)
}

/// The size of one section, zero when absent.
#[must_use]
pub fn section_size(sections: &[Section], name: &str) -> u64 {
    sections
        .iter()
        .find(|section| section.name == name)
        .map_or(0, |section| section.size)
}

/// One symbol of `llvm-nm --size-sort --print-size`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolSize {
    /// Demangled name.
    pub name: String,
    /// Symbol size in bytes.
    pub size: u64,
    /// The nm kind letter (`T`, `t`, `W`, ...).
    pub kind: String,
}

/// Parse `llvm-nm --size-sort --print-size --demangle` (address, size, kind, name).
///
/// # Errors
/// A malformed size or address field.
pub fn parse_nm_sizes(text: &str) -> Result<Vec<SymbolSize>> {
    let mut symbols = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<&str> = trimmed.splitn(4, char::is_whitespace).collect();
        if fields.len() < 4 {
            continue;
        }
        let _address = u64::from_str_radix(fields[0], 16)
            .with_context(|| format!("bad nm address `{}`", fields[0]))?;
        let size = u64::from_str_radix(fields[1], 16)
            .with_context(|| format!("bad nm size `{}`", fields[1]))?;
        let kind = fields[2].to_string();
        let name = fields[3].trim().to_string();
        if name.is_empty() {
            continue;
        }
        symbols.push(SymbolSize { name, size, kind });
    }
    Ok(symbols)
}

/// Dynamic-section facts of one ELF file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynInfo {
    /// Number of `DT_NEEDED` entries.
    pub needed: usize,
    /// `*_JUMP_SLOT` relocations.
    pub jump_slot: usize,
    /// `*_GLOB_DAT` relocations.
    pub glob_dat: usize,
    /// `*_RELATIVE` relocations (not `IRELATIVE`).
    pub relative: usize,
}

/// Count `DT_NEEDED` entries in `llvm-readobj --elf-output-style=GNU -d` output.
#[must_use]
pub fn count_needed(text: &str) -> usize {
    text.lines()
        .filter(|line| line.contains("(NEEDED)"))
        .count()
}

/// Count relocation types in `llvm-readobj --elf-output-style=GNU -r` output.
#[must_use]
pub fn count_relocation(text: &str, suffix: &str) -> usize {
    text.split_whitespace()
        .filter(|token| token.len() > suffix.len() && token.ends_with(suffix))
        .count()
}

/// Count `JUMP_SLOT` relocations.
#[must_use]
pub fn jump_slot_count(text: &str) -> usize {
    count_relocation(text, "_JUMP_SLOT")
}

/// Count `GLOB_DAT` relocations.
#[must_use]
pub fn glob_dat_count(text: &str) -> usize {
    count_relocation(text, "_GLOB_DAT")
}

/// Count `*_RELATIVE` relocations, excluding `*_IRELATIVE`.
#[must_use]
pub fn relative_count(text: &str) -> usize {
    count_relocation(text, "_RELATIVE")
}

/// Parse the dynamic facts from the `-d` and `-r` outputs.
#[must_use]
pub fn dynamic_info(dynamic: &str, relocations: &str) -> DynInfo {
    DynInfo {
        needed: count_needed(dynamic),
        jump_slot: jump_slot_count(relocations),
        glob_dat: glob_dat_count(relocations),
        relative: relative_count(relocations),
    }
}

/// Extract the distinct `libc.a(member)` entries of an lld `-Map` file, in file order.
#[must_use]
pub fn parse_map_members(text: &str) -> Vec<String> {
    let mut seen = BTreeMap::new();
    for line in text.lines() {
        if let Some(open) = line.find("libc.a(") {
            let rest = &line[open + "libc.a(".len()..];
            if let Some(close) = rest.find(')') {
                let member = &rest[..close];
                if !member.is_empty() {
                    seen.insert(member.to_string(), ());
                }
            }
        }
    }
    seen.into_keys().collect()
}

/// One member of an archive as reported by `llvm-size <archive>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberSize {
    /// Member file name.
    pub name: String,
    /// The triple.
    pub size: Berkeley,
}

/// Parse `llvm-size <archive>` output; the member name is the field before ` (ex ...)`.
///
/// # Errors
/// A malformed numeric field.
pub fn parse_archive_sizes(text: &str) -> Result<Vec<MemberSize>> {
    let entries = parse_berkeley(text)?;
    Ok(entries
        .into_iter()
        .map(|entry| MemberSize {
            name: strip_archive_suffix(&entry.name),
            size: entry.size,
        })
        .collect())
}

fn strip_archive_suffix(name: &str) -> String {
    match name.find(" (ex ") {
        Some(index) => name[..index].trim().to_string(),
        None => name.trim().to_string(),
    }
}

/// The size of a file in bytes.
///
/// # Errors
/// An I/O error.
pub fn file_size(path: &Path) -> Result<u64> {
    std::fs::metadata(path)
        .with_context(|| format!("cannot stat {}", path.display()))
        .map(|metadata| metadata.len())
}

/// Parse a decimal or `0x`-prefixed hexadecimal field.
///
/// # Errors
/// A non-numeric field.
pub fn parse_u64(value: &str) -> Result<u64> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix("0x") {
        return u64::from_str_radix(hex, 16).with_context(|| format!("bad number `{value}`"));
    }
    value
        .parse::<u64>()
        .with_context(|| format!("bad number `{value}`"))
}

#[cfg(test)]
#[path = "measure_ut.rs"]
mod tests;
