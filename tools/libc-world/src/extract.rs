// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic extraction of undefined symbols, system headers and `-l` libraries (section 4.6).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::model::elf_machine;

const DROP_PREFIXES: &[&str] = &[".", "$", "__gnu_"];
const DROP_NAMES: &[&str] = &[
    "_GLOBAL_OFFSET_TABLE_",
    "__bss_start",
    "_edata",
    "_end",
    "__dso_handle",
    "_DYNAMIC",
];

/// What one package yielded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Extraction {
    /// Undefined names with `U` (strong) or `w` (weak).
    pub undefined: BTreeMap<String, char>,
    /// System headers from `-MD` files.
    pub headers: BTreeSet<String>,
    /// Libraries named on link lines.
    pub libs: BTreeSet<String>,
    /// Regular files inspected.
    pub files_scanned: u32,
    /// ELF/archive files of the target architecture.
    pub elf_objects: u32,
}

/// Scan the build tree and `dest` for symbols, headers and libraries.
pub fn scan(
    nm: &str,
    roots: &[PathBuf],
    arch: &str,
    sysroot: &Path,
    log_dir: &Path,
) -> Result<Extraction> {
    let Some(machine) = elf_machine(arch) else {
        anyhow::bail!("unknown architecture `{arch}`");
    };
    let mut files = Vec::new();
    for root in roots {
        walk(root, &mut files)?;
    }
    let mut extraction = Extraction::default();
    let mut defined = BTreeSet::new();
    for path in files {
        extraction.files_scanned += 1;
        if !target_object(&path, machine)? {
            continue;
        }
        extraction.elf_objects += 1;
        for (name, kind) in nm_symbols(nm, &path, false)? {
            match kind {
                'U' | 'w' | 'v' => {
                    let marker = if kind == 'U' { 'U' } else { 'w' };
                    let entry = extraction.undefined.entry(name).or_insert(marker);
                    if *entry == 'w' && marker == 'U' {
                        *entry = 'U';
                    }
                }
                _ => {
                    defined.insert(name);
                }
            }
        }
        for (name, _) in nm_symbols(nm, &path, true)? {
            defined.insert(name);
        }
    }
    extraction.undefined.retain(|name, _| {
        !defined.contains(name)
            && !DROP_NAMES.contains(&name.as_str())
            && !DROP_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
    });
    extraction.headers = parse_dependency_dir(&log_dir.join("deps"), sysroot)?;
    extraction.libs = parse_calls(&read_optional(&log_dir.join("calls.log"))?);
    Ok(extraction)
}

fn walk(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .with_context(|| format!("cannot read {}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            walk(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn target_object(path: &Path, machine: u16) -> Result<bool> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 8];
    let read = file.read(&mut magic)?;
    if read >= 4 && magic[..4] == *b"\x7fELF" {
        file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 20];
        file.read_exact(&mut header)?;
        return Ok(elf_header_machine(&header) == Some(machine));
    }
    if read == 8 && magic == *b"!<arch>\n" {
        return archive_matches(&mut file, machine);
    }
    Ok(false)
}

fn elf_header_machine(header: &[u8; 20]) -> Option<u16> {
    if header[..4] != *b"\x7fELF" || header[5] != 1 {
        return None;
    }
    Some(u16::from_le_bytes([header[18], header[19]]))
}

fn archive_matches(file: &mut File, machine: u16) -> Result<bool> {
    file.seek(SeekFrom::Start(8))?;
    let mut any = false;
    loop {
        let mut header = [0u8; 60];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(_) => break,
        }
        let size_text = String::from_utf8_lossy(&header[48..58]).trim().to_string();
        let Ok(size) = size_text.parse::<u64>() else {
            break;
        };
        let data_start = file.stream_position()?;
        let mut magic = [0u8; 4];
        let read = file.read(&mut magic)?;
        if read == 4 && magic == *b"\x7fELF" {
            any = true;
            file.seek(SeekFrom::Start(data_start))?;
            let mut elf = [0u8; 20];
            file.read_exact(&mut elf)?;
            if elf_header_machine(&elf) != Some(machine) {
                return Ok(false);
            }
        }
        let next = data_start + size + (size % 2);
        file.seek(SeekFrom::Start(next))?;
    }
    Ok(any)
}

fn nm_symbols(nm: &str, path: &Path, defined_only: bool) -> Result<Vec<(String, char)>> {
    let mut command = Command::new(nm);
    command.arg("-P");
    if defined_only {
        command.arg("--defined-only");
    } else {
        command.arg("-u");
    }
    command.arg(path);
    let output = command
        .output()
        .with_context(|| format!("cannot run `{nm}`"))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut symbols = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(kind) = parts.next().and_then(|value| value.chars().next()) else {
            continue;
        };
        symbols.push((name.to_string(), kind));
    }
    Ok(symbols)
}

fn read_optional(path: &Path) -> Result<String> {
    if path.is_file() {
        Ok(fs::read_to_string(path)?)
    } else {
        Ok(String::new())
    }
}

/// Parse every `*.d` file in a directory.
pub fn parse_dependency_dir(directory: &Path, sysroot: &Path) -> Result<BTreeSet<String>> {
    let mut headers = BTreeSet::new();
    if !directory.is_dir() {
        return Ok(headers);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    files.sort();
    for file in files {
        if file.extension().is_some_and(|extension| extension == "d") {
            headers.extend(parse_deps(&fs::read_to_string(&file)?, sysroot));
        }
    }
    Ok(headers)
}

/// Parse the Makefile dependency syntax produced by `-MD -MF`.
#[must_use]
pub fn parse_deps(text: &str, sysroot: &Path) -> BTreeSet<String> {
    let joined = text.replace("\\\r\n", " ").replace("\\\n", " ");
    let prefix = format!("{}/usr/include/", sysroot.display());
    let mut headers = BTreeSet::new();
    for line in joined.lines() {
        let Some((_, dependencies)) = line.split_once(':') else {
            continue;
        };
        for token in split_escaped(dependencies) {
            if let Some(header) = token
                .strip_prefix(&prefix)
                .filter(|header| !header.is_empty())
            {
                headers.insert(header.to_string());
            }
        }
    }
    headers
}

fn split_escaped(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            value if value.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Parse `-l` tokens from `calls.log`.
#[must_use]
pub fn parse_calls(text: &str) -> BTreeSet<String> {
    let mut libs = BTreeSet::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("link ") else {
            continue;
        };
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let mut index = 0;
        while index < tokens.len() {
            let token = tokens[index];
            if let Some(name) = token.strip_prefix("-l") {
                if !name.is_empty() {
                    libs.insert(name.to_string());
                } else if let Some(next) =
                    tokens.get(index + 1).filter(|next| !next.starts_with('-'))
                {
                    libs.insert((*next).to_string());
                    index += 1;
                }
            }
            index += 1;
        }
    }
    libs
}

/// Render the `.undef` body (sorted, one `name<TAB>U|w` per line).
#[must_use]
pub fn undef_text(undefined: &BTreeMap<String, char>) -> String {
    let mut text = String::new();
    for (name, marker) in undefined {
        text.push_str(name);
        text.push('\t');
        text.push(*marker);
        text.push('\n');
    }
    text
}

#[cfg(test)]
#[path = "extract_ut.rs"]
mod tests;
