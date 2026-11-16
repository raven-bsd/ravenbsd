// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-owner evidence overlays for the POSIX profile.
//!
//! The seed derives requirement rows from headers, exports and the curated lists. An owner can
//! close a row independently with an overlay under `docs/posix/evidence/<owner>.toml`; the seed and
//! the gate merge supported rows after that derivation. A `present` row needs executable evidence
//! (a test name or command); a `waived` row needs a written waiver in `notes`. An unsupported
//! claim is never applied: the gate reports it as a failure and the seed ignores it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::model::{Meta, OWNERS, Profile, STANDARD, Status};

/// The only statuses an owner overlay may claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayStatus {
    /// The owner asserts the requirement is implemented with executable evidence.
    Present,
    /// The owner explicitly waives the requirement with a written reason.
    Waived,
}

impl OverlayStatus {
    fn target(self) -> Status {
        match self {
            Self::Present => Status::Present,
            Self::Waived => Status::Waived,
        }
    }

    /// Lowercase spelling used in messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Waived => "waived",
        }
    }
}

/// One owner claim.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OverlayRow {
    /// Requirement id from the generated profile.
    pub id: String,
    /// Claimed status.
    pub status: OverlayStatus,
    /// Test name or command for `present`; optional for `waived`.
    #[serde(default)]
    pub evidence: String,
    /// Owner note for `present`; the written waiver for `waived`.
    #[serde(default)]
    pub notes: String,
    /// Owner file that supplied the row; empty for the optional aggregate file.
    #[serde(skip)]
    pub owner: String,
}

#[derive(Deserialize)]
struct OverlayFile {
    schema: u32,
    standard: String,
    #[serde(default, rename = "row")]
    rows: Vec<OverlayRow>,
}

/// One applied overlay row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedRow {
    /// Requirement id.
    pub id: String,
    /// Command/evidence shown by the gate.
    pub command: String,
}

/// One claim that could not be applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedClaim {
    /// Requirement id.
    pub id: String,
    /// Why the claim is unsupported.
    pub reason: String,
}

/// The result of merging an overlay into a profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OverlayOutcome {
    /// Rows that were applied.
    pub applied: Vec<AppliedRow>,
    /// Rows that were rejected and left at their derived status.
    pub unsupported: Vec<UnsupportedClaim>,
}

/// A loaded set of overlay rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    rows: Vec<OverlayRow>,
}

impl Overlay {
    /// Load the optional aggregate file and every `docs/posix/evidence/P1-P5.toml` file.
    ///
    /// Duplicate ids and files outside the P1-P5 owner split are hard errors. A missing overlay
    /// is an empty overlay, so the seed and gate keep working before owners add evidence.
    pub fn load(root: &Path) -> Result<Self> {
        let mut paths: Vec<(PathBuf, String)> = Vec::new();
        let aggregate = root.join("docs/posix/evidence.toml");
        if aggregate.is_file() {
            paths.push((aggregate, String::new()));
        }
        let owner_dir = root.join("docs/posix/evidence");
        if owner_dir.is_dir() {
            let mut owner_paths = Vec::new();
            for entry in std::fs::read_dir(&owner_dir)
                .with_context(|| format!("cannot read {}", owner_dir.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                    continue;
                }
                let owner = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_string();
                if !OWNERS.contains(&owner.as_str()) {
                    bail!(
                        "overlay file `{}` must be named P1.toml through P5.toml",
                        path.display()
                    );
                }
                owner_paths.push((path, owner));
            }
            owner_paths.sort_by(|left, right| left.0.cmp(&right.0));
            paths.extend(owner_paths);
        }

        let mut rows = Vec::new();
        let mut ids = BTreeSet::new();
        for (path, owner) in paths {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            let file: OverlayFile = toml::from_str(&text)
                .with_context(|| format!("cannot parse {}", path.display()))?;
            if file.schema != 1 {
                bail!(
                    "{}: overlay schema {} is not 1",
                    path.display(),
                    file.schema
                );
            }
            if file.standard != STANDARD {
                bail!(
                    "{}: overlay standard `{}` is not `{STANDARD}`",
                    path.display(),
                    file.standard
                );
            }
            for mut row in file.rows {
                if !ids.insert(row.id.clone()) {
                    bail!(
                        "duplicate overlay id `{}` (second occurrence in {})",
                        row.id,
                        path.display()
                    );
                }
                row.owner.clone_from(&owner);
                rows.push(row);
            }
        }
        Ok(Self { rows })
    }

    /// Whether no overlay rows were loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Merge supported rows into `profile`, leaving unsupported claims untouched.
    #[must_use]
    pub fn apply(&self, profile: &mut Profile) -> OverlayOutcome {
        let mut outcome = OverlayOutcome::default();
        for row in &self.rows {
            let Some(meta) = profile.meta_mut(&row.id) else {
                outcome.unsupported.push(UnsupportedClaim {
                    id: row.id.clone(),
                    reason: "no profile row for this overlay id".to_string(),
                });
                continue;
            };
            if !row.owner.is_empty() && meta.owner != row.owner {
                outcome.unsupported.push(UnsupportedClaim {
                    id: row.id.clone(),
                    reason: format!(
                        "owner mismatch: {} file claims a {} row",
                        row.owner, meta.owner
                    ),
                });
                continue;
            }
            match row.status {
                OverlayStatus::Present => {
                    let evidence = row.evidence.trim();
                    if evidence.is_empty() || evidence == "none" {
                        outcome.unsupported.push(UnsupportedClaim {
                            id: row.id.clone(),
                            reason: "present without executable evidence".to_string(),
                        });
                        continue;
                    }
                    apply_present(meta, evidence, row.notes.trim());
                    outcome.applied.push(AppliedRow {
                        id: row.id.clone(),
                        command: evidence.to_string(),
                    });
                }
                OverlayStatus::Waived => {
                    let waiver = row.notes.trim();
                    if waiver.is_empty() {
                        outcome.unsupported.push(UnsupportedClaim {
                            id: row.id.clone(),
                            reason: "waived without a written waiver".to_string(),
                        });
                        continue;
                    }
                    apply_waived(meta, waiver, row.evidence.trim());
                    outcome.applied.push(AppliedRow {
                        id: row.id.clone(),
                        command: "waiver".to_string(),
                    });
                }
            }
        }
        outcome
    }
}

fn apply_present(meta: &mut Meta, evidence: &str, notes: &str) {
    meta.status = Status::Present;
    meta.evidence = evidence.to_string();
    meta.notes = notes.to_string();
    meta.waiver.clear();
}

fn apply_waived(meta: &mut Meta, waiver: &str, evidence: &str) {
    meta.status = Status::Waived;
    meta.evidence = if evidence.is_empty() {
        "none".to_string()
    } else {
        evidence.to_string()
    };
    meta.waiver = waiver.to_string();
    meta.notes.clear();
}

impl OverlayStatus {
    /// The profile status this claim maps to.
    #[must_use]
    pub fn status(self) -> Status {
        self.target()
    }
}
