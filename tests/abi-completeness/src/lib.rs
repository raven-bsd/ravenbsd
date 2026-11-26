// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host completeness check for the documented native syscall ABI.
//!
//! The kernel dispatch surface is parsed the same way the K451 `inventory.py` does: the
//! `raven-abi` enum variants plus every raw `pub const NAME: usize = 0xNN;` that the kernel
//! dispatches before `Syscall::from_usize`. The documented set is read from the management copy
//! `architecture/product-data/syscall-abi.md`; the product docs tree is frozen (AGENTS 31) and is
//! not a fallback. The test asserts equality modulo the machine-readable known-omission register
//! in that same table, so a new omission or a stale register fails instead of silently shipping.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// One dispatch or documented entry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    /// Raw number.
    pub number: u64,
    /// Uppercase name.
    pub name: String,
    /// `path:line` for a dispatch entry; empty for a documented row.
    pub site: String,
}

/// Known omissions keyed by `(number, name)` with their machine-readable reason.
pub type KnownRegister = BTreeMap<(u64, String), String>;

/// The measured model.
#[derive(Clone, Debug, Default)]
pub struct Model {
    /// Kernel dispatch entries.
    pub dispatch: Vec<Entry>,
    /// Documented table rows.
    pub documented: Vec<Entry>,
    /// Explicit reviewer-owned omissions and their machine-readable reason.
    pub known: KnownRegister,
    /// Reserved number ranges, inclusive.
    pub reserved: Vec<(u64, u64)>,
}

/// Repo root from the test crate's `CARGO_MANIFEST_DIR`.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Resolve the management-tree root.
///
/// One policy for every consumer: `RAVEN_MGMT_ROOT` when set and non-empty, otherwise
/// `../ravenbsd-prjmgmt` relative to the product checkout. An empty `env` value means "unset"
/// so tests can exercise the default without mutating the process environment.
pub fn resolve_mgmt_root(product_root: &Path, env: Option<&str>) -> Result<PathBuf, String> {
    let raw = match env {
        Some(value) if !value.is_empty() => Some(value.to_string()),
        Some(_) => None,
        None => std::env::var("RAVEN_MGMT_ROOT")
            .ok()
            .filter(|value| !value.is_empty()),
    };
    let root = match raw {
        Some(value) => PathBuf::from(value),
        None => product_root
            .parent()
            .unwrap_or(product_root)
            .join("ravenbsd-prjmgmt"),
    };
    if !root.is_dir() {
        return Err(format!(
            "RAVEN_MGMT_ROOT: management root {} is not a directory; set RAVEN_MGMT_ROOT to the \
             ravenbsd-prjmgmt checkout (no product docs fallback)",
            root.display()
        ));
    }
    Ok(root)
}

/// Return a required management-tree file, failing loudly when the root/file is absent.
#[must_use]
pub fn mgmt_file(product_root: &Path, relative: &str) -> PathBuf {
    let root = resolve_mgmt_root(product_root, None).unwrap_or_else(|error| panic!("{error}"));
    let path = root.join(relative);
    if !path.is_file() {
        panic!(
            "RAVEN_MGMT_ROOT: missing management copy {}; set RAVEN_MGMT_ROOT to the \
             ravenbsd-prjmgmt checkout (no product docs fallback)",
            path.display()
        );
    }
    path
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn parse_hex(text: &str) -> Option<u64> {
    let digits = text.trim().strip_prefix("0x")?;
    u64::from_str_radix(digits, 16).ok()
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && text.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn parse_const(line: &str) -> Option<(String, u64)> {
    let rest = line.split_once("pub const ")?.1;
    let (name, value) = rest.split_once(": usize = ")?;
    let name = name.trim();
    let value = value.trim().trim_end_matches(';').trim();
    if is_ident(name) {
        parse_hex(value).map(|number| (name.to_string(), number))
    } else {
        None
    }
}

fn parse_enum(line: &str) -> Option<(String, u64)> {
    let line = line.trim();
    let (name, value) = line.split_once('=')?;
    let name = name.trim();
    let value = value.trim().trim_end_matches(';').trim();
    if is_ident(name) && value.starts_with("0x") {
        parse_hex(value).map(|number| (name.to_string(), number))
    } else {
        None
    }
}

fn symbol_table(root: &Path) -> BTreeMap<String, (u64, String)> {
    let mut table = BTreeMap::new();
    let mut paths = Vec::new();
    for directory in [
        root.join("crates/raven-abi/src"),
        root.join("kernel/src/syscall"),
    ] {
        let mut files = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .collect::<Vec<_>>();
        files.sort();
        paths.extend(files);
    }
    for path in paths {
        let text = read(&path);
        for (index, line) in text.lines().enumerate() {
            let site = format!(
                "{}:{}",
                path.strip_prefix(root).unwrap_or(&path).display(),
                index + 1
            );
            if let Some((name, number)) = parse_const(line) {
                table.entry(name).or_insert((number, site.clone()));
            }
            if let Some((name, number)) = parse_enum(line) {
                table.entry(name).or_insert((number, site));
            }
        }
    }
    table
}

fn scan_path_constants(line: &str, table: &BTreeMap<String, (u64, String)>) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = line;
    while let Some(position) = rest.find("::") {
        let after = &rest[position + 2..];
        let name = after
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
            .collect::<String>();
        if is_ident(&name) && table.contains_key(&name) {
            names.push(name);
        }
        rest = &rest[position + 2..];
    }
    names
}

/// Parse the kernel dispatch surface.
#[must_use]
pub fn parse_dispatch(root: &Path) -> Vec<Entry> {
    let table = symbol_table(root);
    let mod_path = root.join("kernel/src/syscall/mod.rs");
    let mod_text = read(&mod_path);
    let mod_site = mod_path
        .strip_prefix(root)
        .unwrap_or(&mod_path)
        .display()
        .to_string();
    let mut found: BTreeMap<String, (u64, String)> = BTreeMap::new();
    let mut fallback = format!("{mod_site}:1");
    for (index, line) in mod_text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        let site = format!("{mod_site}:{}", index + 1);
        if code.contains("Some(call) => basic::dispatch(call, frame)") {
            fallback = site.clone();
        }
        for name in scan_path_constants(code, &table) {
            if let Some((number, _)) = table.get(&name) {
                found.entry(name).or_insert((*number, site.clone()));
            }
        }
        if let Some(after) = code.split_once("Syscall::").map(|(_, after)| after) {
            let name = after
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect::<String>();
            if let Some((number, _)) = table.get(&name) {
                found.entry(name).or_insert((*number, site));
            }
        }
    }
    let basic_path = root.join("kernel/src/syscall/basic.rs");
    let basic_text = read(&basic_path);
    let basic_site = basic_path
        .strip_prefix(root)
        .unwrap_or(&basic_path)
        .display()
        .to_string();
    let mut basic_sites: BTreeMap<String, String> = BTreeMap::new();
    for (index, line) in basic_text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        if let Some(after) = code.split_once("Syscall::").map(|(_, after)| after) {
            let name = after
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect::<String>();
            basic_sites
                .entry(name)
                .or_insert_with(|| format!("{basic_site}:{}", index + 1));
        }
    }
    // Every ABI enum variant is a dispatch entry even when only `Syscall::ALL` mentions it.
    let enum_path = root.join("crates/raven-abi/src/syscall.rs");
    for line in read(&enum_path).lines() {
        if let Some((name, number)) = parse_enum(line) {
            found.entry(name.clone()).or_insert_with(|| {
                (
                    number,
                    basic_sites
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(|| fallback.clone()),
                )
            });
        }
    }
    let mut entries = found
        .into_iter()
        .map(|(name, (number, site))| Entry { number, name, site })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| (a.number, &a.name).cmp(&(b.number, &b.name)));
    entries
}

/// Parse the generated syscall table from the management `syscall-abi.md`.
#[must_use]
pub fn parse_documented(root: &Path) -> Vec<Entry> {
    let path = mgmt_file(root, "architecture/product-data/syscall-abi.md");
    let text = read(&path);
    let mut in_table = false;
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with("## Syscalls") {
            in_table = true;
            continue;
        }
        if in_table && line.starts_with("## ") {
            break;
        }
        if !in_table || !line.trim_start().starts_with("| `0x") {
            continue;
        }
        let fields = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        let Some(number) = fields.first().and_then(|f| parse_hex(f.trim_matches('`'))) else {
            continue;
        };
        let Some(name) = fields
            .get(1)
            .map(|f| f.trim_matches('`').to_string())
            .filter(|name| is_ident(name))
        else {
            continue;
        };
        rows.push(Entry {
            number,
            name,
            site: String::new(),
        });
    }
    rows
}

/// Parse the known-omission and reserved-range register in the management `syscall-abi.md`.
#[must_use]
pub fn parse_register(root: &Path) -> (KnownRegister, Vec<(u64, u64)>) {
    let path = mgmt_file(root, "architecture/product-data/syscall-abi.md");
    let text = read(&path);
    let mut known: KnownRegister = BTreeMap::new();
    let mut reserved = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line
            .split_once("<!-- abi-completeness: known-omission ")
            .map(|(_, rest)| rest)
        {
            let body = rest.split("-->").next().unwrap_or("").trim();
            let mut parts = body.splitn(3, |c: char| c.is_whitespace());
            if let (Some(number), Some(name), Some(reason)) =
                (parts.next(), parts.next(), parts.next())
                && let Some(number) = parse_hex(number)
                && is_ident(name)
            {
                known.insert((number, name.to_string()), reason.trim().to_string());
            }
        }
        if let Some(rest) = line
            .split_once("<!-- abi-completeness: reserved ")
            .map(|(_, rest)| rest)
        {
            let body = rest.split("-->").next().unwrap_or("").trim();
            let mut parts = body.split_whitespace();
            if let (Some(first), Some(last)) = (parts.next(), parts.next())
                && let (Some(first), Some(last)) = (parse_hex(first), parse_hex(last))
            {
                reserved.push((first.min(last), first.max(last)));
            }
        }
    }
    (known, reserved)
}

/// Load the full model.
#[must_use]
pub fn model(root: &Path) -> Model {
    let dispatch = parse_dispatch(root);
    let documented = parse_documented(root);
    let (known, reserved) = parse_register(root);
    Model {
        dispatch,
        documented,
        known,
        reserved,
    }
}

fn duplicates(entries: &[Entry]) -> Vec<u64> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.number) {
            duplicates.insert(entry.number);
        }
    }
    duplicates.into_iter().collect()
}

fn in_reserved(number: u64, ranges: &[(u64, u64)]) -> bool {
    ranges
        .iter()
        .any(|(first, last)| number >= *first && number <= *last)
}

/// Run every completeness rule and return the human-readable failures.
#[must_use]
pub fn check(model: &Model) -> Vec<String> {
    let mut errors = Vec::new();
    for number in duplicates(&model.dispatch) {
        errors.push(format!("duplicate dispatch number {number:#04x}"));
    }
    for number in duplicates(&model.documented) {
        errors.push(format!("duplicate documented number {number:#04x}"));
    }
    let dispatch = model
        .dispatch
        .iter()
        .map(|entry| (entry.number, entry.name.clone()))
        .collect::<BTreeSet<_>>();
    let documented = model
        .documented
        .iter()
        .map(|entry| (entry.number, entry.name.clone()))
        .collect::<BTreeSet<_>>();
    let known = model.known.keys().cloned().collect::<BTreeSet<_>>();

    for (name, number) in [("PROC_CHROOT", 0x0D), ("PROC_REBOOT", 0x46)] {
        let key = (number, name.to_string());
        if !dispatch.contains(&key) {
            errors.push(format!(
                "focus {name} ({number:#04x}) is not in the dispatch surface"
            ));
        }
        if !documented.contains(&key) {
            errors.push(format!(
                "focus {name} ({number:#04x}) is not in the documented table"
            ));
        }
        if known.contains(&key) {
            errors.push(format!(
                "focus {name} ({number:#04x}) is still registered as a known omission"
            ));
        }
    }
    for (key, reason) in &model.known {
        if reason.is_empty() {
            errors.push(format!(
                "known omission {:#04x} {} has no machine-readable reason",
                key.0, key.1
            ));
        } else if !(reason.starts_with("public:") || reason.starts_with("private:")) {
            errors.push(format!(
                "known omission {:#04x} {} has an unrecognised reason `{reason}`",
                key.0, key.1
            ));
        }
    }

    for entry in model.dispatch.iter() {
        let key = (entry.number, entry.name.clone());
        if !documented.contains(&key) && !known.contains(&key) {
            errors.push(format!(
                "missing from the documented table: {:#04x} {} (dispatch {})",
                entry.number, entry.name, entry.site
            ));
        }
    }
    for entry in model.documented.iter() {
        let key = (entry.number, entry.name.clone());
        if !dispatch.contains(&key) {
            errors.push(format!(
                "documented but not dispatched: {:#04x} {}",
                entry.number, entry.name
            ));
        }
    }
    for key in model.known.keys() {
        if documented.contains(key) || !dispatch.contains(key) {
            errors.push(format!(
                "stale known omission: {:#04x} {} is now documented or absent from dispatch",
                key.0, key.1
            ));
        }
    }
    let maximum = model
        .dispatch
        .iter()
        .chain(model.documented.iter())
        .map(|entry| entry.number)
        .max()
        .unwrap_or(0);
    let used = dispatch
        .iter()
        .chain(documented.iter())
        .map(|(number, _)| *number)
        .collect::<BTreeSet<_>>();
    for number in 0..=maximum {
        if !used.contains(&number) && !in_reserved(number, &model.reserved) {
            errors.push(format!("undocumented gap at {number:#04x}"));
        }
    }
    errors
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
