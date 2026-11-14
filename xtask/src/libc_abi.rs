// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `cargo xtask libc-abi`: the offline RAVEN_1.0 symbol freeze (C68).
//!
//! The command consumes C33's census, the shell `Symbol.map`, the non-pending `alias64.toml`
//! rows and `data-symbols.toml`; it writes the version chain, the flat public/private lists,
//! the frozen record, GAPS.md and RECONCILE.md. It never runs git, a network tool or a compiler.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::libc_lint::Violation;

/// A parsed shell export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// Symbol name.
    pub name: String,
    /// Version node.
    pub node: String,
    /// Architectures (`all` or a comma list).
    pub arches: String,
}

/// A parsed census row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CensusRow {
    /// Name.
    pub name: String,
    /// Ring, or 4 for private.
    pub ring: u8,
    /// Header path.
    pub header: String,
    /// `func`, `data`, `macro`, ...
    pub kind: String,
    /// Ring-3 stub description.
    pub stub: String,
    /// Aliases.
    pub aliases: Vec<String>,
    /// Owning crate.
    pub krate: String,
    /// Area.
    pub area: String,
    /// Status.
    pub status: String,
    /// Gap description.
    pub gap: Vec<String>,
}

/// The census and its completeness.
#[derive(Clone, Debug, Default)]
pub struct Census {
    /// `complete = true` required by `gen`.
    pub complete: bool,
    /// Revision.
    pub census_rev: String,
    /// Rows by name.
    pub rows: BTreeMap<String, CensusRow>,
}

/// A data-symbol row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataRow {
    /// Name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// C type.
    pub type_: String,
    /// Aliases.
    pub aliases: Vec<String>,
    /// Source note.
    pub source: String,
    /// Reviewer verification.
    pub verified: bool,
}

/// The generated text files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    /// `Versions.def`.
    pub versions: String,
    /// `Symbols.list`.
    pub symbols: String,
    /// `Symbols.private`.
    pub private: String,
    /// `frozen/RAVEN_1.0.symbols`.
    pub frozen: String,
}

/// Parse a `Symbol.map` in the shell/FreeBSD shape.
///
/// # Errors
/// A malformed block or symbol line.
pub fn parse_symbol_map(text: &str) -> Result<Vec<Export>> {
    let mut exports = Vec::new();
    let mut node: Option<String> = None;
    let mut local = false;
    for (index, raw) in text.lines().enumerate() {
        let line = strip_comments(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains('{') {
            let name = line
                .split('{')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if name.is_empty() {
                bail!("Symbol.map:{}: empty node name", index + 1);
            }
            node = Some(name);
            local = false;
            continue;
        }
        if line.starts_with('}') {
            node = None;
            local = false;
            continue;
        }
        if line == "global:" {
            local = false;
            continue;
        }
        if line == "local:" {
            local = true;
            continue;
        }
        if local || line == "*;" {
            continue;
        }
        let Some(name) = line.strip_suffix(';') else {
            bail!("Symbol.map:{}: missing `;`", index + 1);
        };
        let name = name.trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            bail!("Symbol.map:{}: invalid symbol", index + 1);
        }
        let Some(node) = &node else {
            bail!("Symbol.map:{}: symbol outside a node", index + 1);
        };
        let arches = arch_comment(raw);
        exports.push(Export {
            name: name.to_string(),
            node: node.clone(),
            arches,
        });
    }
    Ok(exports)
}

fn strip_comments(line: &str) -> String {
    let mut out = String::new();
    let mut in_block = false;
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if in_block {
            if line.get(index..index + 2) == Some("*/") {
                in_block = false;
                index = index.saturating_add(2);
            } else {
                index = index.saturating_add(1);
            }
            continue;
        }
        if line.get(index..index + 2) == Some("/*") {
            in_block = true;
            index = index.saturating_add(2);
            continue;
        }
        let byte = bytes[index];
        if byte == b'#' {
            break;
        }
        out.push(byte as char);
        index = index.saturating_add(1);
    }
    out
}

fn arch_comment(raw: &str) -> String {
    let Some((_, comment)) = raw.split_once('#') else {
        return "all".to_string();
    };
    let words = comment
        .split_whitespace()
        .filter(|word| matches!(*word, "x86_64" | "aarch64" | "riscv64"))
        .collect::<Vec<_>>();
    if words.is_empty() {
        "all".to_string()
    } else {
        words.join(",")
    }
}

/// Parse the census.
///
/// # Errors
/// A TOML or field error.
pub fn parse_census(text: &str) -> Result<Census> {
    let value: toml::Value = toml::from_str(text).context("census TOML")?;
    let mut census = Census {
        complete: value
            .get("complete")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        census_rev: value
            .get("census_rev")
            .and_then(toml::Value::as_str)
            .unwrap_or("unversioned")
            .to_string(),
        rows: BTreeMap::new(),
    };
    let Some(rows) = value.get("symbol").and_then(toml::Value::as_array) else {
        return Ok(census);
    };
    for row in rows {
        let Some(name) = row.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let ring = match row.get("ring") {
            Some(toml::Value::Integer(value)) => u8::try_from(*value).unwrap_or(4),
            Some(toml::Value::String(value)) if value == "private" => 4,
            _ => 4,
        };
        let mut parsed = CensusRow {
            name: name.to_string(),
            ring,
            header: row
                .get("header")
                .and_then(toml::Value::as_str)
                .unwrap_or("-")
                .to_string(),
            kind: row
                .get("kind")
                .and_then(toml::Value::as_str)
                .unwrap_or("func")
                .to_string(),
            stub: row
                .get("stub")
                .and_then(toml::Value::as_str)
                .unwrap_or("")
                .to_string(),
            aliases: string_array(row.get("aliases")),
            krate: row
                .get("crate")
                .and_then(toml::Value::as_str)
                .unwrap_or("UNOWNED")
                .to_string(),
            area: row
                .get("area")
                .and_then(toml::Value::as_str)
                .unwrap_or("-")
                .to_string(),
            status: row
                .get("status")
                .and_then(toml::Value::as_str)
                .unwrap_or("")
                .to_string(),
            gap: string_array(row.get("gap")),
        };
        // The census uses `personality` and `families`; keep the renderer independent of them.
        parsed.gap.sort();
        census.rows.insert(name.to_string(), parsed);
    }
    Ok(census)
}

fn string_array(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(toml::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `data-symbols.toml`.
///
/// # Errors
/// A TOML error.
pub fn parse_data_symbols(text: &str) -> Result<BTreeMap<String, DataRow>> {
    let value: toml::Value = toml::from_str(text).context("data-symbols TOML")?;
    let mut rows = BTreeMap::new();
    let Some(data) = value.get("data").and_then(toml::Value::as_array) else {
        return Ok(rows);
    };
    for row in data {
        let Some(name) = row.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let size = row
            .get("size")
            .and_then(toml::Value::as_integer)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        let entry = DataRow {
            name: name.to_string(),
            size,
            type_: row
                .get("type")
                .and_then(toml::Value::as_str)
                .unwrap_or("-")
                .to_string(),
            aliases: string_array(row.get("aliases")),
            source: row
                .get("source")
                .and_then(toml::Value::as_str)
                .unwrap_or("-")
                .to_string(),
            verified: row
                .get("verified")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
        };
        for alias in &entry.aliases {
            rows.insert(alias.clone(), entry.clone());
        }
        rows.insert(name.to_string(), entry);
    }
    Ok(rows)
}

/// The derived public/private sets and records.
#[derive(Clone, Debug, Default)]
pub struct Model {
    /// Public names sorted.
    pub public: Vec<String>,
    /// Private names sorted.
    pub private: Vec<String>,
    /// Frozen records, one per public name.
    pub records: Vec<String>,
    /// Export-only public names.
    pub export_only: Vec<String>,
    /// Census-only public names (gaps).
    pub census_only: Vec<String>,
}

/// Build the model from the census and shell exports.
///
/// # Errors
/// A missing data size for a data export, or an incomplete census.
pub fn derive(
    census: &Census,
    exports: &[Export],
    data: &BTreeMap<String, DataRow>,
) -> Result<Model> {
    let mut public = BTreeSet::new();
    let mut private = BTreeSet::new();
    let mut export_only = BTreeSet::new();
    let mut arches: BTreeMap<&str, String> = BTreeMap::new();
    for export in exports {
        let is_private = export.node.starts_with("RAVENprivate")
            || export.name.starts_with("__rl_")
            || export.name.starts_with("__raven_")
            || export.name.starts_with("_rtld_")
            || export.name == "r_debug_state";
        if is_private {
            private.insert(export.name.clone());
        } else {
            public.insert(export.name.clone());
            if !census.rows.contains_key(&export.name) {
                export_only.insert(export.name.clone());
            }
            let entry = arches.entry(export.name.as_str()).or_default();
            if entry.is_empty() {
                entry.push_str(&export.arches);
            } else if export.arches == "all" || entry != "all" {
                let mut set = entry.split(',').collect::<BTreeSet<_>>();
                for arch in export.arches.split(',') {
                    set.insert(arch);
                }
                *entry = set.into_iter().collect::<Vec<_>>().join(",");
            }
        }
    }
    let mut records = Vec::new();
    for name in &public {
        let row = census
            .rows
            .get(name)
            .or_else(|| data.get(name).and_then(|data| census.rows.get(&data.name)));
        let Some(row) = row else {
            continue;
        };
        if !matches!(row.kind.as_str(), "func" | "data") {
            continue;
        }
        let (kind, size) = if row.kind == "data" {
            let Some(data) = data.get(name) else {
                bail!("data-size-missing: {name}");
            };
            ("D", data.size.to_string())
        } else {
            ("F", "-".to_string())
        };
        let arches = arches.get(name.as_str()).map_or("all", String::as_str);
        records.push(format!(
            "{name} {kind} {size} {arches} {} {}",
            row.ring.min(3),
            if row.header.is_empty() {
                "-"
            } else {
                &row.header
            }
        ));
    }
    records.sort();
    let mut census_only = Vec::new();
    for (name, row) in &census.rows {
        if (0..=2).contains(&row.ring) && row.status != "SKIP" && !public.contains(name) {
            census_only.push(name.clone());
        }
    }
    Ok(Model {
        public: public.into_iter().collect(),
        private: private.into_iter().collect(),
        records,
        export_only: export_only.into_iter().collect(),
        census_only,
    })
}

/// Render all generated files.
#[must_use]
pub fn render(model: &Model, census: &Census) -> Rendered {
    let mut symbols = header("Generated by cargo xtask libc-abi gen; do not edit.");
    // The header is comment-only and C60's list parser skips `#` lines. The canonical combined
    // list follows: public names, then the private marker and the private names.
    symbols.push_str("# static libc.a and shared libc.so.1 export the same public symbols.\n");
    symbols.push_str("# private names (RAVENprivate_1.0) follow the private marker.\n");
    for name in &model.public {
        symbols.push_str(name);
        symbols.push('\n');
    }
    // C60's `mkmap check` still consumes the canonical combined list: public names, then the
    // private marker and the private names. `Symbols.private` is the informational split.
    symbols.push_str("# private\n");
    for name in &model.private {
        symbols.push_str(name);
        symbols.push('\n');
    }
    let mut private = header("Generated by cargo xtask libc-abi gen; do not edit.");
    for name in &model.private {
        private.push_str(name);
        private.push('\n');
    }
    let versions = "# SPDX-License-Identifier: BSD-3-Clause\n# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n#\n# Version nodes of libc.so.1; RAVENprivate_1.0 is always last.\n\nRAVEN_1.0 { };\nRAVENprivate_1.0 { } RAVEN_1.0;\n".to_string();
    let mut frozen = format!(
        "# node: RAVEN_1.0\n# status: candidate\n# census_rev: {}\n# records: {}\n",
        census.census_rev,
        model.records.len()
    );
    for record in &model.records {
        frozen.push_str(record);
        frozen.push('\n');
    }
    Rendered {
        versions,
        symbols,
        private,
        frozen,
    }
}

fn header(comment: &str) -> String {
    format!(
        "# SPDX-License-Identifier: BSD-3-Clause\n# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n# {comment}\n"
    )
}

/// Write the generated files; with `check`, compare only.
///
/// # Errors
/// An I/O error or a difference.
pub fn write_or_check(root: &Path, rendered: &Rendered, check: bool) -> Result<Vec<String>> {
    let base = root.join("libc/ravenlibc/abi");
    let files = [
        (base.join("Versions.def"), &rendered.versions),
        (base.join("Symbols.list"), &rendered.symbols),
        (base.join("Symbols.private"), &rendered.private),
        (base.join("frozen/RAVEN_1.0.symbols"), &rendered.frozen),
    ];
    let mut differences = Vec::new();
    for (path, text) in files {
        if check {
            let current = fs::read_to_string(&path).unwrap_or_default();
            if current != *text {
                differences.push(path.display().to_string());
            }
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
        }
    }
    Ok(differences)
}

/// Load the real inputs.
///
/// # Errors
/// A missing shell, census or data file.
pub fn load(root: &Path) -> Result<(Census, Vec<Export>, BTreeMap<String, DataRow>)> {
    let census_path = root.join("libc/ravenlibc/abi/census/symbols.toml");
    let census_text = fs::read_to_string(&census_path)
        .with_context(|| format!("cannot read {}", census_path.display()))?;
    let census = parse_census(&census_text)?;
    let map_path = root.join("libc/ravenlibc/crates/raven-libc/Symbol.map");
    let map_text = fs::read_to_string(&map_path)
        .with_context(|| format!("cannot read {}", map_path.display()))?;
    let mut exports = parse_symbol_map(&map_text)?;
    // Merge the personality export tables.
    let mut dirs = Vec::new();
    let crates = root.join("libc/ravenlibc/crates");
    if let Ok(entries) = fs::read_dir(&crates) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }
    }
    dirs.sort();
    for dir in dirs {
        let path = dir.join("exports.toml");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let value: toml::Value =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        if let Some(rows) = value.get("export").and_then(toml::Value::as_array) {
            for row in rows {
                if let (Some(name), Some(version)) = (
                    row.get("name").and_then(toml::Value::as_str),
                    row.get("version").and_then(toml::Value::as_str),
                ) {
                    exports.push(Export {
                        name: name.to_string(),
                        node: version.to_string(),
                        arches: "all".to_string(),
                    });
                }
            }
        }
    }
    let data_path = root.join("libc/ravenlibc/abi/data-symbols.toml");
    let data = if data_path.is_file() {
        parse_data_symbols(&fs::read_to_string(&data_path)?)?
    } else {
        BTreeMap::new()
    };
    Ok((census, exports, data))
}

/// Run one `libc-abi` command.
///
/// # Errors
/// Usage or I/O.
pub fn run(root: &Path, args: &[String]) -> Result<u8> {
    let Some(command) = args.first().map(String::as_str) else {
        bail!("usage: libc-abi gen|check|gaps|reconcile|release");
    };
    let mut check = false;
    let mut root = root.to_path_buf();
    let mut index = 1usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--root") => {
                index = index.saturating_add(1);
                root = PathBuf::from(args.get(index).context("--root value")?);
            }
            Some("--check") => check = true,
            Some("--json") | Some("--node") | Some("--reviewer") | Some("--fail-on-gap") => {
                index = index.saturating_add(1);
            }
            Some(value) if value.starts_with('-') => bail!("unknown libc-abi flag `{value}`"),
            _ => {}
        }
        index = index.saturating_add(1);
    }
    let (census, exports, data) = load(&root)?;
    if !census.complete && matches!(command, "gen" | "check") {
        eprintln!("libc-abi: census incomplete");
        return Ok(2);
    }
    let model = derive(&census, &exports, &data)?;
    let rendered = render(&model, &census);
    match command {
        "gen" => {
            let differences = write_or_check(&root, &rendered, check)?;
            if differences.is_empty() {
                eprintln!("libc-abi: ok ({} records)", model.records.len());
                Ok(0)
            } else {
                for path in differences {
                    eprintln!("libc-abi: differs: {path}");
                }
                Ok(1)
            }
        }
        "check" => {
            let differences = write_or_check(&root, &rendered, true)?;
            let mut failed = !differences.is_empty();
            let frozen = root.join("libc/ravenlibc/abi/frozen/RAVEN_1.0.symbols");
            if let Ok(current) = fs::read_to_string(&frozen) {
                for (key, message) in audit_frozen(&rendered.frozen, &current) {
                    eprintln!("libc-abi: [{key}] {message}");
                    failed = true;
                }
            }
            if failed {
                Ok(1)
            } else {
                eprintln!("libc-abi: ok ({} records)", model.records.len());
                Ok(0)
            }
        }
        "gaps" => {
            let path = root.join("libc/ravenlibc/abi/GAPS.md");
            fs::write(&path, gaps_markdown(&census, &model))
                .with_context(|| format!("cannot write {}", path.display()))?;
            eprintln!("libc-abi: gaps: {} census-only", model.census_only.len());
            Ok(0)
        }
        "reconcile" => {
            let path = root.join("libc/ravenlibc/abi/RECONCILE.md");
            fs::write(&path, reconcile_markdown(&model))
                .with_context(|| format!("cannot write {}", path.display()))?;
            eprintln!(
                "libc-abi: reconcile: {} census-only, {} export-only",
                model.census_only.len(),
                model.export_only.len()
            );
            Ok(0)
        }
        "release" => Ok(2),
        other => bail!("unknown libc-abi command `{other}`"),
    }
}

fn gaps_markdown(census: &Census, model: &Model) -> String {
    let mut out = String::from(
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->\n<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n# Gaps\n\n",
    );
    for ring in 0..=2 {
        let names = model
            .census_only
            .iter()
            .filter(|name| census.rows.get(*name).is_some_and(|row| row.ring == ring))
            .count();
        out.push_str(&format!("- ring {ring}: {names}\n"));
    }
    out.push_str("\n| name | ring | header | crate | area |\n|---|---|---|---|---|\n");
    for name in &model.census_only {
        let Some(row) = census.rows.get(name) else {
            continue;
        };
        out.push_str(&format!(
            "| {name} | {} | {} | {} | {} |\n",
            row.ring, row.header, row.krate, row.area
        ));
    }
    out
}

fn reconcile_markdown(model: &Model) -> String {
    let mut out = String::from(
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->\n<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n# Reconcile\n\n",
    );
    out.push_str(&format!("- census_only: {}\n", model.census_only.len()));
    out.push_str(&format!("- export_only: {}\n", model.export_only.len()));
    out.push_str(&format!(
        "- both: {}\n",
        model.public.len() - model.export_only.len()
    ));
    out
}

/// Compare a frozen `.symbols` file with the expected records.
#[must_use]
pub fn audit_frozen(expected: &str, actual: &str) -> Vec<(String, String)> {
    let expected = frozen_records(expected);
    let actual = frozen_records(actual);
    let mut findings = Vec::new();
    for (name, fields) in &expected {
        let Some(have) = actual.get(name) else {
            findings.push((
                "frozen-missing".to_string(),
                format!("{name} is frozen but absent"),
            ));
            continue;
        };
        for (index, key) in ["kind", "size", "arch", "ring"].iter().enumerate() {
            if fields.get(index + 1) != have.get(index + 1) {
                findings.push((
                    format!("frozen-{key}"),
                    format!(
                        "{name}: frozen {:?}, now {:?}",
                        fields.get(index + 1),
                        have.get(index + 1)
                    ),
                ));
            }
        }
    }
    for name in actual.keys() {
        if !expected.contains_key(name) {
            findings.push((
                "frozen-new".to_string(),
                format!("{name} is not in the frozen node"),
            ));
        }
    }
    findings
}

fn frozen_records(text: &str) -> BTreeMap<String, Vec<String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let fields = line
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>();
            Some((fields.first()?.clone(), fields))
        })
        .collect()
}

/// The lint-check adapter.
///
/// # Errors
/// An I/O error.
pub fn lint_frozen(root: &Path) -> Result<Vec<Violation>> {
    let Ok((census, exports, data)) = load(root) else {
        return Ok(Vec::new());
    };
    if !census.complete {
        return Ok(Vec::new());
    }
    let model = derive(&census, &exports, &data)?;
    let rendered = render(&model, &census);
    let path = root.join("libc/ravenlibc/abi/frozen/RAVEN_1.0.symbols");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let current = fs::read_to_string(&path)?;
    let mut findings = audit_frozen(&rendered.frozen, &current);
    let txt = root.join("libc/ravenlibc/abi/frozen/RAVEN_1.0.txt");
    if let Ok(text) = fs::read_to_string(&txt) {
        let names = text
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter_map(|line| line.split_whitespace().next().map(str::to_string))
            .collect::<BTreeSet<_>>();
        let expected = model.public.iter().cloned().collect::<BTreeSet<_>>();
        if names != expected {
            findings.push((
                "frozen-txt".to_string(),
                "mkmap .txt and .symbols name sets differ".to_string(),
            ));
        }
    }
    Ok(findings
        .into_iter()
        .map(|(key, message)| {
            crate::libc_lint::error(
                "abi-freeze",
                &key,
                "libc/ravenlibc/abi/frozen/RAVEN_1.0.symbols",
                0,
                message,
            )
        })
        .collect())
}

#[cfg(test)]
#[path = "libc_abi_ut.rs"]
mod tests;
