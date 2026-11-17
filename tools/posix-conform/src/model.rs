// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Profile and traceability data structures plus the validation rules.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The only standard this profile speaks.
pub const STANDARD: &str = "posix-2024";
/// The five implementation owners P1-P5.
pub const OWNERS: [&str; 5] = ["P1", "P2", "P3", "P4", "P5"];

/// One requirement status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The requirement is absent.
    Missing,
    /// Present but not yet proven or complete.
    Partial,
    /// Implemented with file-level evidence.
    Present,
    /// Explicitly omitted by an owner decision.
    Waived,
}

impl Status {
    /// Lowercase spelling used in reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Partial => "partial",
            Self::Present => "present",
            Self::Waived => "waived",
        }
    }
}

/// Fields every profile row carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// Stable requirement id.
    pub id: String,
    /// Standard spelling.
    pub standard: String,
    /// Requirement status.
    pub status: Status,
    /// Test name or file-level evidence; `none` is explicit.
    pub evidence: String,
    /// Implementation owner, P1-P5.
    pub owner: String,
    /// Written owner waiver for `waived` rows.
    #[serde(default)]
    pub waiver: String,
    /// Short note; identifiers only, never standard prose.
    #[serde(default)]
    pub notes: String,
    /// Platform-gap ids that track the requirement.
    #[serde(default)]
    pub pg: Vec<String>,
}

/// One standard header and its required names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeaderRow {
    /// Shared fields.
    #[serde(flatten)]
    pub meta: Meta,
    /// Header path, for example `sys/stat.h`.
    pub name: String,
    /// Required macros/constants.
    #[serde(default)]
    pub required_macros: Vec<String>,
    /// Required types.
    #[serde(default)]
    pub required_types: Vec<String>,
    /// Required functions.
    #[serde(default)]
    pub required_functions: Vec<String>,
}

/// One system interface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InterfaceRow {
    /// Shared fields.
    #[serde(flatten)]
    pub meta: Meta,
    /// Interface name.
    pub name: String,
    /// Declaring header.
    pub header: String,
    /// `func`, `macro` or `data`.
    pub kind: String,
    /// Whether POSIX requires it.
    pub required: bool,
    /// Type or declaration constraint, when one is recorded.
    #[serde(default)]
    pub signature: String,
}

/// One required utility.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UtilityRow {
    /// Shared fields.
    #[serde(flatten)]
    pub meta: Meta,
    /// Utility name.
    pub name: String,
    /// Required option set, in our own words.
    #[serde(default)]
    pub options: String,
    /// Exit-status requirement, in our own words.
    #[serde(default)]
    pub exit_status: String,
    /// Locale basics.
    #[serde(default)]
    pub locale: String,
    /// Test name, or `none`.
    #[serde(default)]
    pub test: String,
}

/// One shell feature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShellRow {
    /// Shared fields.
    #[serde(flatten)]
    pub meta: Meta,
    /// Feature name.
    pub name: String,
    /// Requirement summary in our own words.
    pub description: String,
}

/// One filesystem, process or tty behaviour row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BehaviorRow {
    /// Shared fields.
    #[serde(flatten)]
    pub meta: Meta,
    /// Behaviour name.
    pub name: String,
    /// Requirement summary in our own words.
    pub description: String,
}

/// Platform gate status. x86-64 is the first gate; the other targets are pending their qtest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlatformStatus {
    /// x86-64 gate state.
    pub x86_64: String,
    /// aarch64 gate state.
    pub aarch64: String,
    /// riscv64 gate state.
    pub riscv64: String,
}

impl PlatformStatus {
    /// The first-target policy: x86-64 gate, the other platforms pending.
    #[must_use]
    pub fn new() -> Self {
        Self {
            x86_64: "gate".to_string(),
            aarch64: "pending".to_string(),
            riscv64: "pending".to_string(),
        }
    }
}

impl Default for PlatformStatus {
    fn default() -> Self {
        Self::new()
    }
}

/// The complete profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Profile schema.
    pub schema: u32,
    /// Standard spelling.
    pub standard: String,
    /// Platform gate status.
    #[serde(default)]
    pub platform: PlatformStatus,
    /// Headers.
    #[serde(default)]
    pub header: Vec<HeaderRow>,
    /// Interfaces.
    #[serde(default)]
    pub interface: Vec<InterfaceRow>,
    /// Utilities.
    #[serde(default)]
    pub utility: Vec<UtilityRow>,
    /// Shell features.
    #[serde(default)]
    pub shell_feature: Vec<ShellRow>,
    /// Filesystem behaviours.
    #[serde(default)]
    pub filesystem: Vec<BehaviorRow>,
    /// Process behaviours.
    #[serde(default)]
    pub process: Vec<BehaviorRow>,
    /// TTY behaviours.
    #[serde(default)]
    pub tty: Vec<BehaviorRow>,
}

/// One traceability row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceRow {
    /// Requirement id.
    pub id: String,
    /// Implementation ticket (P1-P5).
    pub ticket: String,
    /// Test name, or `none`.
    pub test: String,
    /// Evidence file, or `none`.
    pub evidence: String,
}

/// The traceability file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceFile {
    /// Trace schema.
    pub schema: u32,
    /// Standard spelling.
    pub standard: String,
    /// Rows.
    #[serde(default)]
    pub trace: Vec<TraceRow>,
}

impl Profile {
    /// Every row's shared fields.
    #[must_use]
    pub fn metas(&self) -> Vec<(&str, &Meta)> {
        let mut rows = Vec::new();
        rows.extend(self.header.iter().map(|row| ("header", &row.meta)));
        rows.extend(self.interface.iter().map(|row| ("interface", &row.meta)));
        rows.extend(self.utility.iter().map(|row| ("utility", &row.meta)));
        rows.extend(
            self.shell_feature
                .iter()
                .map(|row| ("shell_feature", &row.meta)),
        );
        rows.extend(self.filesystem.iter().map(|row| ("filesystem", &row.meta)));
        rows.extend(self.process.iter().map(|row| ("process", &row.meta)));
        rows.extend(self.tty.iter().map(|row| ("tty", &row.meta)));
        rows
    }

    /// Find one row's shared fields by id.
    pub fn meta_mut(&mut self, id: &str) -> Option<&mut Meta> {
        if let Some(row) = self.header.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        if let Some(row) = self.interface.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        if let Some(row) = self.utility.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        if let Some(row) = self.shell_feature.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        if let Some(row) = self.filesystem.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        if let Some(row) = self.process.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        if let Some(row) = self.tty.iter_mut().find(|row| row.meta.id == id) {
            return Some(&mut row.meta);
        }
        None
    }

    /// All ids, rejecting duplicates.
    pub fn unique_ids(&self) -> Result<BTreeSet<String>> {
        let mut ids = BTreeSet::new();
        for (_, meta) in self.metas() {
            if !ids.insert(meta.id.clone()) {
                bail!("duplicate requirement id `{}`", meta.id);
            }
        }
        Ok(ids)
    }

    /// Count rows by area and status.
    #[must_use]
    pub fn counts(&self) -> BTreeMap<&'static str, [usize; 4]> {
        let mut counts = BTreeMap::new();
        for (area, meta) in self.metas() {
            let entry = counts
                .entry(match area {
                    "header" => "headers",
                    "interface" => "interfaces",
                    "utility" => "utilities",
                    "shell_feature" => "shell",
                    "filesystem" => "filesystem",
                    "process" => "process",
                    "tty" => "tty",
                    _ => "other",
                })
                .or_insert([0usize; 4]);
            match meta.status {
                Status::Missing => entry[0] += 1,
                Status::Partial => entry[1] += 1,
                Status::Present => entry[2] += 1,
                Status::Waived => entry[3] += 1,
            }
        }
        counts
    }
}

/// Validate one profile row.
fn validate_meta(area: &str, meta: &Meta, errors: &mut Vec<String>) {
    if meta.id.trim().is_empty() {
        errors.push(format!("{area}: empty id"));
    }
    if meta.standard != STANDARD {
        errors.push(format!(
            "{}: standard `{}` is not `{STANDARD}`",
            meta.id, meta.standard
        ));
    }
    if !OWNERS.contains(&meta.owner.as_str()) {
        errors.push(format!("{}: unknown owner `{}`", meta.id, meta.owner));
    }
    if meta.evidence.trim().is_empty() {
        errors.push(format!("{}: missing evidence", meta.id));
    }
    if meta.status == Status::Waived && meta.waiver.trim().is_empty() {
        errors.push(format!("{}: waived without a reason", meta.id));
    }
    if meta.status != Status::Waived && !meta.waiver.trim().is_empty() {
        errors.push(format!("{}: waiver set on a non-waived row", meta.id));
    }
    for pg in &meta.pg {
        if !pg.starts_with("PG-") || pg.len() < 4 {
            errors.push(format!("{}: invalid pg id `{pg}`", meta.id));
        }
    }
}

/// Read the current exported symbol names.
pub fn read_symbols(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join("libc/ravenlibc/abi/Symbols.list");
    let mut symbols = BTreeSet::new();
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        symbols.insert(line.to_string());
    }
    Ok(symbols)
}

/// One census ring-0 row: name, kind, standard tags and optional header.
pub type Ring0Row = (String, String, Vec<String>, Option<String>);

/// Read the donor census and return `(name, kind, standard tags, header)` for ring-0 rows.
pub fn read_ring0(root: &Path) -> Result<Vec<Ring0Row>> {
    let path = root.join("libc/ravenlibc/abi/census/symbols.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
    let symbols = value
        .get("symbol")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("census has no [[symbol]] rows"))?;
    let mut rows = Vec::new();
    for symbol in symbols {
        let ring = symbol.get("ring").and_then(toml::Value::as_integer);
        if ring != Some(0) {
            continue;
        }
        let Some(name) = symbol.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let kind = symbol
            .get("kind")
            .and_then(toml::Value::as_str)
            .unwrap_or("func");
        let standard = symbol
            .get("standard")
            .and_then(toml::Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let header = symbol
            .get("header")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        rows.push((name.to_string(), kind.to_string(), standard, header));
    }
    Ok(rows)
}

/// Whether a census row is part of the POSIX/C/XSI requirement set.
#[must_use]
pub fn is_posix_relevant(tags: &[String]) -> bool {
    if tags.is_empty() {
        return true;
    }
    tags.iter().any(|tag| {
        matches!(
            tag.as_str(),
            "posix" | "posix-2024" | "xsi" | "xpg" | "iso-c" | "iso-c11" | "ipv6-basic-api"
        )
    })
}

/// Validate the profile, traceability and current classifications.
pub fn validate(root: &Path, profile: &Profile, trace: &TraceFile) -> Result<()> {
    let mut errors = Vec::new();
    if profile.schema != 1 {
        errors.push(format!("profile schema {} is not 1", profile.schema));
    }
    if profile.standard != STANDARD {
        errors.push(format!(
            "profile standard `{}` is not `{STANDARD}`",
            profile.standard
        ));
    }
    if trace.schema != 1 {
        errors.push(format!("trace schema {} is not 1", trace.schema));
    }
    if trace.standard != STANDARD {
        errors.push(format!(
            "trace standard `{}` is not `{STANDARD}`",
            trace.standard
        ));
    }
    for (platform, state) in [
        ("x86_64", &profile.platform.x86_64),
        ("aarch64", &profile.platform.aarch64),
        ("riscv64", &profile.platform.riscv64),
    ] {
        if state != "gate" && state != "pending" {
            errors.push(format!("platform {platform} has unknown state `{state}`"));
        }
    }
    if profile.platform.x86_64 != "gate" {
        errors.push("x86_64 must be the first gate platform".to_string());
    }
    for (area, meta) in profile.metas() {
        validate_meta(area, meta, &mut errors);
    }
    if let Err(error) = profile.unique_ids() {
        errors.push(error.to_string());
    }

    let ids: BTreeSet<String> = profile
        .metas()
        .into_iter()
        .map(|(_, meta)| meta.id.clone())
        .collect();
    let mut trace_ids = BTreeSet::new();
    for row in &trace.trace {
        if !trace_ids.insert(row.id.clone()) {
            errors.push(format!("duplicate trace id `{}`", row.id));
        }
        if !ids.contains(&row.id) {
            errors.push(format!("trace id `{}` has no profile row", row.id));
        }
        if !OWNERS.contains(&row.ticket.as_str()) {
            errors.push(format!(
                "trace id `{}` has unknown ticket `{}`",
                row.id, row.ticket
            ));
        }
        if row.test.trim().is_empty() {
            errors.push(format!("trace id `{}` has an empty test", row.id));
        }
        if row.evidence.trim().is_empty() {
            errors.push(format!("trace id `{}` has empty evidence", row.id));
        }
    }
    for id in &ids {
        if !trace_ids.contains(id) {
            errors.push(format!("profile id `{id}` has no trace row"));
        }
    }

    // Header/interface evidence must match the current headers and frozen exports.
    let header_root = root.join("libc/ravenlibc/include");
    let symbols = match read_symbols(root) {
        Ok(symbols) => symbols,
        Err(error) => {
            errors.push(error.to_string());
            BTreeSet::new()
        }
    };
    for row in &profile.header {
        if row.meta.status != Status::Missing && !header_root.join(&row.name).is_file() {
            errors.push(format!(
                "header `{}` is {} but the header file is absent",
                row.name,
                row.meta.status.as_str()
            ));
        }
    }
    for row in &profile.interface {
        if matches!(row.meta.status, Status::Present | Status::Partial)
            && !symbols.contains(&row.name)
        {
            errors.push(format!(
                "interface `{}` is {} but not exported",
                row.name,
                row.meta.status.as_str()
            ));
        }
    }

    // Every POSIX-relevant current export must be classified.
    let mut classified = BTreeSet::new();
    for row in &profile.interface {
        classified.insert(row.name.clone());
    }
    for row in &profile.header {
        classified.extend(row.required_macros.iter().cloned());
        classified.extend(row.required_types.iter().cloned());
        classified.extend(row.required_functions.iter().cloned());
    }
    if let Ok(rows) = read_ring0(root) {
        for (name, _kind, tags, _header) in rows {
            if is_posix_relevant(&tags) && symbols.contains(&name) && !classified.contains(&name) {
                errors.push(format!("current export `{name}` is not classified"));
            }
        }
    }

    // Utility status comes from the evidence overlays, not from the presence of a source
    // directory: a directory can exist with a stub or a non-conforming subset. Present and
    // partial rows must still name existing evidence, which is checked below.
    for row in &profile.utility {
        if !matches!(row.meta.status, Status::Present | Status::Partial)
            || row.meta.evidence == "none"
        {
            continue;
        }
        let evidence = row
            .meta
            .evidence
            .strip_prefix("test:")
            .unwrap_or(&row.meta.evidence);
        if evidence.starts_with("qtest:") {
            continue;
        }
        if !root.join(evidence).exists() {
            errors.push(format!(
                "utility `{}` evidence `{}` is absent",
                row.name, row.meta.evidence
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("\n"))
    }
}

fn qtest_for(id: &str) -> &'static str {
    match id {
        "process.fork" => "tests/qtest/cases/proc-fork.toml",
        "filesystem.access" | "filesystem.permission" => "tests/qtest/cases/perm.toml",
        "filesystem.ownership"
        | "filesystem.special-file"
        | "filesystem.statvfs"
        | "filesystem.sync"
        | "filesystem.timestamp" => "tests/qtest/cases/fsd-ufs-rw.toml",
        "filesystem.directory"
        | "filesystem.hard-link"
        | "filesystem.mkdir-rmdir"
        | "filesystem.mount"
        | "filesystem.path-resolution"
        | "filesystem.rename"
        | "filesystem.stat-family"
        | "filesystem.symlink"
        | "filesystem.truncate" => "tests/qtest/cases/posix-fs.toml",
        _ => "none",
    }
}

/// Build a deterministic traceability file from a profile.
#[must_use]
pub fn trace_from_profile(profile: &Profile) -> TraceFile {
    let mut trace = Vec::new();
    for (_, meta) in profile.metas() {
        let test = qtest_for(&meta.id).to_string();
        trace.push(TraceRow {
            id: meta.id.clone(),
            ticket: meta.owner.clone(),
            test,
            evidence: meta.evidence.clone(),
        });
    }
    trace.sort_by(|left, right| left.id.cmp(&right.id));
    TraceFile {
        schema: 1,
        standard: STANDARD.to_string(),
        trace,
    }
}

/// Load a TOML profile.
pub fn load_profile(path: &Path) -> Result<Profile> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// Load a TOML traceability file.
pub fn load_trace(path: &Path) -> Result<TraceFile> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// Serialize a profile with the required SPDX header.
pub fn render_profile(profile: &Profile) -> Result<String> {
    let body = toml::to_string(profile).context("cannot serialize profile")?;
    Ok(format!(
        "# SPDX-License-Identifier: BSD-3-Clause\n\
         # Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\
         # GENERATED by tools/posix-conform seed; edit the seed inputs, not this file.\n\
         {body}"
    ))
}

/// Serialize a traceability file with the required SPDX header.
pub fn render_trace(trace: &TraceFile) -> Result<String> {
    let body = toml::to_string(trace).context("cannot serialize traceability")?;
    Ok(format!(
        "# SPDX-License-Identifier: BSD-3-Clause\n\
         # Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\
         # GENERATED by tools/posix-conform seed; edit the seed inputs, not this file.\n\
         {body}"
    ))
}
