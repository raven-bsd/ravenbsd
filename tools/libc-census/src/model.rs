// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Data types shared by the census stages.
//!
//! Every type that reaches a file is serialised with explicit field order and
//! sorted collections so two runs produce identical bytes.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// The nine donors in the column order of the doc 15 census table.
pub const DONORS: [&str; 9] = [
    "glibc", "musl", "freebsd", "netbsd", "openbsd", "bionic", "illumos", "mlibc", "picolibc",
];

/// The six libcs that enter `n6` (doc 15 3.B.2).
pub const N6_DONORS: [&str; 6] = ["glibc", "musl", "freebsd", "openbsd", "illumos", "bionic"];

/// Family membership of every donor (02 section 1).
pub fn donor_family(donor: &str) -> Option<&'static str> {
    match donor {
        "glibc" | "musl" | "bionic" => Some("linux"),
        "freebsd" | "netbsd" | "openbsd" => Some("bsd"),
        "illumos" => Some("solaris"),
        _ => None,
    }
}

/// The kind of a symbol.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    /// A function or a function-like macro.
    #[default]
    #[serde(rename = "func")]
    Func,
    /// A data object.
    #[serde(rename = "data")]
    Data,
    /// An object-like macro.
    #[serde(rename = "macro")]
    Macro,
    /// A header without a linkable symbol.
    #[serde(rename = "header-only")]
    HeaderOnly,
}

impl Kind {
    /// Parse the spelling used by the fixtures and the generated files.
    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "func" => Ok(Self::Func),
            "data" => Ok(Self::Data),
            "macro" => Ok(Self::Macro),
            "header-only" => Ok(Self::HeaderOnly),
            other => bail!("unknown symbol kind `{other}`"),
        }
    }

    /// Whether the variable regex must be used for this kind.
    pub fn is_data(self) -> bool {
        matches!(self, Self::Data)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Func => "func",
            Self::Data => "data",
            Self::Macro => "macro",
            Self::HeaderOnly => "header-only",
        })
    }
}

/// One donor cell: `D/H` file counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    /// Non-header files that match.
    pub d: u32,
    /// Header files that match.
    pub h: u32,
}

impl Cell {
    /// Total number of matching files.
    pub fn total(self) -> u32 {
        self.d.saturating_add(self.h)
    }

    /// The `D/H` text used by doc 15.
    pub fn text(self) -> String {
        format!("{}/{}", self.d, self.h)
    }
}

/// One row of `cells.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRow {
    /// Symbol name.
    pub name: String,
    /// Symbol kind.
    pub kind: Kind,
    /// Per-donor cells, every configured donor present.
    pub cells: BTreeMap<String, Cell>,
    /// Number of the six reference libcs that define the name.
    pub n6: u32,
    /// Donors with `D + H > 0`, sorted.
    pub libcs: Vec<String>,
    /// Families with at least one donor, sorted.
    pub families: Vec<String>,
}

/// The ring value used by `symbols.toml`: an integer or `"private"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Ring {
    /// Ring 0, 1, 2 or 3.
    Number(u8),
    /// The private-version-node marker.
    Private(String),
}

impl Ring {
    /// Build a ring from a TOML integer or the literal `private`.
    pub fn parse_number(value: u8) -> Self {
        Self::Number(value)
    }

    /// Build the only accepted string form.
    pub fn private() -> Self {
        Self::Private("private".to_string())
    }

    /// Validate a ring read from `assignments.toml`.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Number(0..=3) => Ok(()),
            Self::Number(other) => bail!("ring {other} is outside 0..=3"),
            Self::Private(text) if text == "private" => Ok(()),
            Self::Private(text) => bail!("unsupported ring string `{text}`"),
        }
    }

    /// The numeric ring, or `None` for the private marker.
    pub fn number(&self) -> Option<u8> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Private(_) => None,
        }
    }
}

/// One `[[symbol]]` row of `symbols.toml`.
///
/// The declaration order is the contract order of ticket C33 section 4.5.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolRow {
    /// Symbol name.
    pub name: String,
    /// Final ring: the reviewed assignment, else the computed ring.
    pub ring: Option<Ring>,
    /// Installed header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Version node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Symbol kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Standard tags, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standard: Vec<String>,
    /// Families with a donor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub families: Vec<String>,
    /// Personality of a single-family ring-2 symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    /// Extra exported spellings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Owning logic crate.
    #[serde(rename = "crate", skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
    /// Donor-map area number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<u32>,
    /// Implementation status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Release wave.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave: Option<u32>,
    /// Warning policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<String>,
    /// Ring-3 stub behaviour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub: Option<String>,
    /// Kernel/server dependencies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gap: Vec<String>,
    /// Donors that define the name, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libcs: Vec<String>,
    /// Number of the six reference libcs that define the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n6: Option<u32>,
    /// The mechanical ring before assignments.
    pub ring_computed: u8,
    /// `assignment` or `computed`.
    pub ring_source: String,
    /// Reviewer reason when the assignment wins over a different computed ring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ring_override: Option<String>,
}

/// One `[[assign]]` row of `assignments.toml` (reviewer-owned input).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    /// Symbol name.
    pub name: String,
    /// Reviewed ring.
    pub ring: Option<Ring>,
    /// Personality for a ring-2 row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    /// Owning logic crate.
    #[serde(rename = "crate", skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
    /// Donor-map area number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<u32>,
    /// Implementation status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Installed header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Release wave.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave: Option<u32>,
    /// Warning policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn: Option<String>,
    /// Ring-3 stub behaviour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub: Option<String>,
    /// Kernel/server dependencies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gap: Vec<String>,
    /// Mandatory reason when the assignment changes the computed ring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ring_override: Option<String>,
    /// Symbol kind override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Extra exported spellings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// The whole `assignments.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignments {
    /// Schema version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<u32>,
    /// The rows.
    #[serde(default)]
    pub assign: Vec<Assignment>,
}

/// One standard entry mined from the glibc manual.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandardEntry {
    /// Mapped standard tags, sorted.
    pub standard: Vec<String>,
    /// Installed header, when the manual gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Function, data or macro.
    pub kind: String,
}

/// `standards.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandardsOutput {
    /// Entries keyed by name.
    pub entries: BTreeMap<String, StandardEntry>,
}

/// One list parsed from a donor.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymlistSummary {
    /// Number of names.
    pub count: usize,
    /// Names, sorted.
    pub names: Vec<String>,
}

/// `symlists.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymlistsOutput {
    /// One summary per source.
    pub sources: BTreeMap<String, SymlistSummary>,
}

/// Per-snapshot status.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    /// Number of packages found.
    pub packages: usize,
    /// `skipped: <reason>` when the snapshot was not given or not found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// Classification of one package.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSummary {
    /// Categories that fired, sorted.
    pub categories: Vec<String>,
    /// Number of patch files inspected.
    pub patches: usize,
}

/// `ports.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortsOutput {
    /// One entry per snapshot flag.
    pub snapshots: BTreeMap<String, SnapshotSummary>,
    /// Per-package result.
    pub packages: BTreeMap<String, PackageSummary>,
    /// Symbol package counts from `libc-symbol`.
    pub symbols: BTreeMap<String, usize>,
    /// Header package counts from `libc-header`.
    pub headers: BTreeMap<String, usize>,
    /// Macro package counts from `libc-macro`.
    pub macros: BTreeMap<String, usize>,
}

/// One library and the packages that link it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryUse {
    /// Number of packages.
    pub packages: usize,
    /// Whether `-lNAME` is subsumed by ravenlibc and ships as a stub.
    pub stub: bool,
}

/// `uses.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsesOutput {
    /// `skipped: no sources tree` or absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Header package counts.
    pub headers: BTreeMap<String, usize>,
    /// Library package counts.
    pub libraries: BTreeMap<String, LibraryUse>,
    /// Textual call-site package counts.
    pub symbols: BTreeMap<String, usize>,
    /// Undefined-symbol data (S2), when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undef: Option<UndefOutput>,
    /// Number of package directories seen.
    pub packages: usize,
}

/// `llvm-nm -u` classification per symbol.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndefOutput {
    /// `skipped: no undefined-symbol lists` or absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Strong (`U`) package counts.
    pub strong: BTreeMap<String, usize>,
    /// Weak (`w`) package counts.
    pub weak: BTreeMap<String, usize>,
}

/// `run-summary.json`: what the run found, skipped and counted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    /// Inputs that existed.
    pub inputs_found: Vec<String>,
    /// Inputs that were absent or not given.
    pub inputs_missing: Vec<String>,
    /// `donors.lock` revisions for the donors that have one.
    pub donors: BTreeMap<String, String>,
    /// Missing root paths per donor (relative to the resources directory).
    pub missing_roots: BTreeMap<String, Vec<String>>,
    /// Matching source files per donor.
    pub files: BTreeMap<String, usize>,
    /// Files skipped because of a NUL byte.
    pub skipped_binary: usize,
    /// Files skipped because they exceed 16 MiB.
    pub skipped_large: usize,
    /// Snapshot status lines.
    pub snapshots: BTreeMap<String, String>,
    /// Undefined-symbol status line.
    pub undef: String,
    /// False when any donor or root was missing.
    pub complete: bool,
    /// The revision stamp written to `symbols.toml`.
    pub census_rev: String,
}

/// One tolerated difference from the doc 15 table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDifference {
    /// Table key.
    pub key: String,
    /// Donor column.
    pub donor: String,
    /// Value printed by doc 15.
    pub doc: String,
    /// Value produced by the tool.
    pub tool: String,
    /// Why the difference is tolerated.
    pub reason: String,
}

/// `known-differences.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDifferences {
    /// The tolerated rows.
    #[serde(default)]
    pub difference: Vec<KnownDifference>,
}

/// The CLI exit code contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Exit {
    /// Success.
    #[default]
    Ok,
    /// A gate failed.
    GateFailed,
    /// Usage or I/O error.
    Usage,
}

impl Exit {
    /// Numeric form for `std::process::ExitCode`.
    pub fn code(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::GateFailed => 1,
            Self::Usage => 2,
        }
    }
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
