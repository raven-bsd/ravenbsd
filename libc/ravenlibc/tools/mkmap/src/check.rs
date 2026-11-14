// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Rules K1-K7 of `donor-map/14` 3.A.7 against a built `libc.a` (and optionally `libc.so.1`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::Finding;
use crate::merge::{Merged, merge, parse_list};
use crate::tools::{ArchiveSymbol, DynSymbol, count_jump_slots};
use crate::versions::{Node, NodeBody, parse_symbol_map, parse_versions};

/// Everything one `check` needs.
pub struct CheckOptions {
    /// Workspace root (tool working directory).
    pub root: PathBuf,
    /// `build/<arch>/libc.a`.
    pub archive: PathBuf,
    /// The committed `abi/Symbols.list`.
    pub list: PathBuf,
    /// Every `Symbol.map` to merge.
    pub maps: Vec<PathBuf>,
    /// `abi/Versions.def`.
    pub versions: PathBuf,
    /// `hooks.toml`.
    pub hooks: PathBuf,
    /// The `expected/` directory.
    pub expected: PathBuf,
    /// The architecture being checked.
    pub arch: String,
    /// The optional shared object for K7.
    pub so: Option<PathBuf>,
    /// The `nm` program.
    pub nm: String,
    /// The `readelf` program.
    pub readelf: String,
}

/// The summary printed after a passing check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Every archive member.
    pub members: usize,
    /// Members of the shell crate.
    pub shell_members: usize,
    /// Public names.
    pub public: usize,
    /// Private names.
    pub private: usize,
}

#[derive(Debug, Deserialize)]
struct HookFile {
    #[serde(default)]
    hook: Vec<HookRow>,
}

#[derive(Debug, Deserialize)]
struct HookRow {
    name: String,
    #[serde(default)]
    strong_symbol: Option<String>,
    #[serde(default)]
    strong_file: String,
    #[serde(default)]
    pulled_by: Vec<String>,
}

/// Run every rule.
///
/// # Errors
/// A missing file or a tool that cannot run; a rule violation is returned in the vector.
pub fn run(options: &CheckOptions) -> Result<(Vec<Finding>, Summary)> {
    let versions = read_versions(&options.versions)?;
    let mut bodies: Vec<NodeBody> = Vec::new();
    for path in &options.maps {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let source = path.display().to_string();
        bodies.extend(parse_symbol_map(&text, &source)?);
    }
    let merged = merge(&versions, &bodies, &options.arch)?;
    let mut findings = Vec::new();

    // K1: the committed list equals the regenerated one, with no duplicates.
    let list_text = std::fs::read_to_string(&options.list)
        .with_context(|| format!("cannot read {}", options.list.display()))?;
    let (list_public, list_private) = parse_list(&list_text);
    check_k1(&list_public, &list_private, &merged, &mut findings);

    let archive = run_nm(options)?;
    let summary = Summary {
        members: archive
            .iter()
            .map(|symbol| symbol.member.clone())
            .collect::<BTreeSet<_>>()
            .len(),
        shell_members: shell_members(&archive).len(),
        public: merged.public.len(),
        private: merged.private.len(),
    };
    check_k2_k3_k4(&archive, &merged, &mut findings);
    check_k5(&archive, &options.hooks, &merged, &mut findings)?;
    check_k6(&options.expected, &merged, &mut findings)?;
    if let Some(so) = &options.so {
        check_k7(so, options, &merged, &mut findings)?;
    }
    Ok((findings, summary))
}

fn read_versions(path: &Path) -> Result<Vec<Node>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_versions(&text)
}

fn run_nm(options: &CheckOptions) -> Result<Vec<ArchiveSymbol>> {
    let program = options.nm.clone();
    let archive = options.archive.display().to_string();
    let output = crate::tools::run(
        &program,
        &[
            "--defined-only",
            "--extern-only",
            "--format=posix",
            &archive,
        ],
        &options.root,
    )?;
    let symbols = crate::tools::parse_nm(&output);
    if symbols.is_empty() {
        bail!("`{program}` found no defined global symbol in {archive}");
    }
    Ok(symbols)
}

fn check_k1(
    list_public: &[String],
    list_private: &[String],
    merged: &Merged,
    findings: &mut Vec<Finding>,
) {
    for name in list_public {
        if !merged.public.iter().any(|candidate| candidate == name) {
            findings.push(Finding::new(
                "K1",
                format!("`{name}` is listed but not in Symbol.map"),
            ));
        }
    }
    for name in &merged.public {
        if !list_public.iter().any(|candidate| candidate == name) {
            findings.push(Finding::new(
                "K1",
                format!("`{name}` is missing from Symbols.list"),
            ));
        }
    }
    for name in list_private {
        if !merged.private.iter().any(|candidate| candidate == name) {
            findings.push(Finding::new(
                "K1",
                format!("private `{name}` is listed but not in Symbol.map"),
            ));
        }
    }
    for name in &merged.private {
        if !list_private.iter().any(|candidate| candidate == name) {
            findings.push(Finding::new(
                "K1",
                format!("private `{name}` is missing from Symbols.list"),
            ));
        }
    }
    let unique = list_public.iter().collect::<BTreeSet<_>>().len();
    if unique != list_public.len() {
        findings.push(Finding::new(
            "K1",
            "Symbols.list has duplicate public names",
        ));
    }
    let private_unique = list_private.iter().collect::<BTreeSet<_>>().len();
    if private_unique != list_private.len() {
        findings.push(Finding::new(
            "K1",
            "Symbols.list has duplicate private names",
        ));
    }
}

/// Hidden entry points that the kernel resumes at; they are internal to a signal dependency,
/// not shell exports, and deliberately have no `Symbol.map` row.
const INTERNAL_SIGNAL_SYMBOLS: &[&str] = &["__raven_sigentry"];

fn is_shell_member(member: &str) -> bool {
    let base = member.rsplit('/').next().unwrap_or(member);
    // A staticlib member is `<crate>-<hash>.<crate>.<hash>-cgu.N.rcgu.o`; some rustc
    // versions omit the leading `lib` for a staticlib. Both spellings are the shell crate.
    base.starts_with("raven_libc.")
        || base.starts_with("raven_libc-")
        || base.starts_with("libraven_libc.")
        || base.starts_with("libraven_libc-")
}

fn shell_members(symbols: &[ArchiveSymbol]) -> BTreeSet<String> {
    symbols
        .iter()
        .filter(|symbol| is_shell_member(&symbol.member))
        .map(|symbol| symbol.member.clone())
        .collect()
}

fn is_mangled(name: &str) -> bool {
    (name.starts_with("_ZN") && name.ends_with('E'))
        || name.starts_with("_R")
        || name.starts_with("anon.")
}

fn check_k2_k3_k4(archive: &[ArchiveSymbol], merged: &Merged, findings: &mut Vec<Finding>) {
    let public = merged
        .public
        .iter()
        .chain(merged.private.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut definitions: BTreeMap<&str, Vec<&ArchiveSymbol>> = BTreeMap::new();
    for symbol in archive {
        definitions.entry(&symbol.name).or_default().push(symbol);
    }

    for name in &merged.public {
        let Some(list) = definitions.get(name.as_str()) else {
            findings.push(Finding::new(
                "K2",
                format!("public `{name}` is not defined"),
            ));
            continue;
        };
        let mut strong = 0usize;
        for symbol in list {
            if !is_shell_member(&symbol.member) {
                findings.push(Finding::new(
                    "K2",
                    format!(
                        "`{name}` is defined by non-shell member `{}`",
                        symbol.member
                    ),
                ));
            } else if symbol.is_weak() {
                findings.push(Finding::new(
                    "K2",
                    format!(
                        "public `{name}` has a weak definition in `{}`",
                        symbol.member
                    ),
                ));
            } else {
                strong = strong.saturating_add(1);
            }
        }
        if strong != 1 {
            findings.push(Finding::new(
                "K2",
                format!("public `{name}` has {strong} strong definitions"),
            ));
        }
    }

    for symbol in archive {
        if is_shell_member(&symbol.member) {
            if public.contains(&symbol.name)
                || symbol.name.starts_with("__rl_")
                || INTERNAL_SIGNAL_SYMBOLS.contains(&symbol.name.as_str())
                || is_mangled(&symbol.name)
            {
                continue;
            }
            findings.push(Finding::new(
                "K3",
                format!(
                    "`{}` is defined by shell member `{}` but is not in Symbol.map",
                    symbol.name, symbol.member
                ),
            ));
        } else if public.contains(&symbol.name) {
            findings.push(Finding::new(
                "K3",
                format!(
                    "public `{}` is also defined by non-shell member `{}`",
                    symbol.name, symbol.member
                ),
            ));
        }
    }

    let mut text: BTreeMap<&str, usize> = BTreeMap::new();
    let mut data: BTreeMap<&str, usize> = BTreeMap::new();
    for symbol in archive {
        if !is_shell_member(&symbol.member) {
            continue;
        }
        // K4 protects the C namespace: a Rust-mangled item or an LLVM internal global cannot
        // collide with a user's C symbol, so only C-visible names count (C60 report, K4 note).
        if is_mangled(&symbol.name) {
            continue;
        }
        if symbol.is_text() {
            *text.entry(symbol.member.as_str()).or_default() += 1;
        } else if symbol.is_data() {
            *data.entry(symbol.member.as_str()).or_default() += 1;
        }
    }
    for (member, count) in text {
        if count > 4 {
            findings.push(Finding::new(
                "K4",
                format!("`{member}` defines {count} global text symbols (max 4)"),
            ));
        }
    }
    for (member, count) in data {
        if count > 1 {
            findings.push(Finding::new(
                "K4",
                format!("`{member}` defines {count} global data symbols (max 1)"),
            ));
        }
    }
}

fn check_k5(
    archive: &[ArchiveSymbol],
    hooks_path: &Path,
    merged: &Merged,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    let text = std::fs::read_to_string(hooks_path)
        .with_context(|| format!("cannot read {}", hooks_path.display()))?;
    let file: HookFile =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", hooks_path.display()))?;
    let public = merged
        .public
        .iter()
        .chain(merged.private.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    for row in &file.hook {
        let strong = row
            .strong_symbol
            .clone()
            .unwrap_or_else(|| row.name.clone());
        let strong_members = archive
            .iter()
            .filter(|symbol| symbol.name == strong && !symbol.is_weak())
            .map(|symbol| symbol.member.clone())
            .collect::<BTreeSet<_>>();
        if strong_members.is_empty() {
            findings.push(Finding::new(
                "K5",
                format!("hook `{}` has no strong definition `{strong}`", row.name),
            ));
            continue;
        }
        if strong_members.len() > 1 {
            findings.push(Finding::new(
                "K5",
                format!(
                    "hook `{}` is defined strongly in {} members",
                    row.name,
                    strong_members.len()
                ),
            ));
        }
        for pulled in &row.pulled_by {
            if !public.contains(pulled) {
                continue;
            }
            for member in archive
                .iter()
                .filter(|symbol| &symbol.name == pulled)
                .map(|symbol| &symbol.member)
            {
                if strong_members.contains(member) {
                    findings.push(Finding::new(
                        "K5",
                        format!(
                            "hook `{}` and its caller `{pulled}` share member `{member}`",
                            row.name
                        ),
                    ));
                }
            }
        }
        if row.strong_file.is_empty() {
            findings.push(Finding::new(
                "K5",
                format!("hook `{}` has no strong_file", row.name),
            ));
        }
    }
    Ok(())
}

fn check_k6(expected_dir: &Path, merged: &Merged, findings: &mut Vec<Finding>) -> Result<()> {
    let known = merged
        .public
        .iter()
        .chain(merged.private.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut deferred: BTreeMap<String, String> = BTreeMap::new();
    let deferred_path = expected_dir.join("DEFERRED.txt");
    if deferred_path.is_file() {
        let text = std::fs::read_to_string(&deferred_path)
            .with_context(|| format!("cannot read {}", deferred_path.display()))?;
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.splitn(2, char::is_whitespace);
            let name = fields.next().unwrap_or("");
            let reason = fields.next().unwrap_or("").trim();
            if reason.is_empty() {
                findings.push(Finding::new(
                    "K6",
                    format!(
                        "{}:{}: `{name}` is deferred without a reason",
                        number + 1,
                        deferred_path.display()
                    ),
                ));
                continue;
            }
            deferred.insert(name.to_string(), reason.to_string());
        }
    }
    let entries = std::fs::read_dir(expected_dir)
        .with_context(|| format!("cannot read {}", expected_dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("txt") {
            continue;
        }
        if path.file_name().and_then(|value| value.to_str()) == Some("DEFERRED.txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if known.contains(line) || deferred.contains_key(line) {
                continue;
            }
            findings.push(Finding::new(
                "K6",
                format!(
                    "`{line}` of {} is neither public nor deferred",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
            ));
        }
    }
    Ok(())
}

fn check_k7(
    so: &Path,
    options: &CheckOptions,
    merged: &Merged,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    let program = options.readelf.clone();
    let path = so.display().to_string();
    let dyn_syms = crate::tools::run(&program, &["--dyn-syms", "-W", &path], &options.root)?;
    let symbols = crate::tools::parse_dyn_syms(&dyn_syms);
    let defined = symbols
        .iter()
        .filter(|symbol| !symbol.name.is_empty())
        .collect::<Vec<&DynSymbol>>();
    for symbol in &defined {
        if symbol.version.is_empty() {
            findings.push(Finding::new(
                "K7",
                format!("`{}` is unversioned in {path}", symbol.name),
            ));
        }
        if !merged
            .public
            .iter()
            .chain(merged.private.iter())
            .any(|name| name == &symbol.name)
        {
            findings.push(Finding::new(
                "K7",
                format!(
                    "`{}` is exported by {path} but not in Symbol.map",
                    symbol.name
                ),
            ));
        }
    }
    for name in &merged.public {
        if !defined.iter().any(|symbol| &symbol.name == name) {
            findings.push(Finding::new(
                "K7",
                format!("public `{name}` is missing from {path}"),
            ));
        }
    }
    let relocations = crate::tools::run(&program, &["-rW", &path], &options.root)?;
    let jumps = count_jump_slots(&relocations);
    if jumps > 40 {
        findings.push(Finding::new(
            "K7",
            format!("{path} has {jumps} JUMP_SLOT relocations (max 40)"),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "check_ut.rs"]
mod tests;
