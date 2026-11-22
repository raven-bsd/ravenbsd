// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-owner evidence overlays for the POSIX profile.
//!
//! The seed derives requirement rows from headers, exports and the curated lists. An owner can
//! close a row independently with an overlay under `docs/posix/evidence/<owner>.toml`; the seed
//! applies structurally valid rows after that derivation and the gate re-applies them and executes
//! the evidence. A `present` row needs structured evidence (`qtest:<case>` or
//! `test:<repo-relative-path>`); a `waived` row needs a written waiver in `notes`. A missing or
//! failing evidence target rejects the claim; it never becomes a pass.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

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
    /// The profile status this claim maps to.
    #[must_use]
    pub fn status(self) -> Status {
        match self {
            Self::Present => Status::Present,
            Self::Waived => Status::Waived,
        }
    }
}

/// Structured evidence for a `present` claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// `qtest:<case>`: `tests/qtest/cases/<case>.toml`, executed by the gate.
    Qtest(String),
    /// `test:<repo-relative-path>`: executed by the runner declared in the file.
    Test(PathBuf),
}

impl Evidence {
    /// The evidence spelling stored in the profile and gate report.
    #[must_use]
    pub fn command(&self) -> String {
        match self {
            Self::Qtest(case) => format!("qtest:{case}"),
            Self::Test(path) => format!("test:{}", path.display()),
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
    /// Structured evidence for `present`; optional for `waived`.
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

/// One structurally valid claim the gate must execute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    /// A qtest case the gate runs with `cargo xtask qtest --case <case>`.
    Qtest {
        /// Requirement id.
        id: String,
        /// Case stem under `tests/qtest/cases`.
        case: String,
    },
    /// A repository test the gate runs through its declared runner.
    Test {
        /// Requirement id.
        id: String,
        /// Repository-relative path.
        path: PathBuf,
    },
    /// A written owner waiver; it stays visible in the gate row.
    Waived {
        /// Requirement id.
        id: String,
        /// Written waiver text.
        waiver: String,
    },
}

impl Claim {
    /// The requirement id this claim closes.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Qtest { id, .. } | Self::Test { id, .. } | Self::Waived { id, .. } => id,
        }
    }

    /// The evidence spelling for the gate report.
    #[must_use]
    pub fn command(&self) -> String {
        match self {
            Self::Qtest { case, .. } => format!("qtest:{case}"),
            Self::Test { path, .. } => format!("test:{}", path.display()),
            Self::Waived { .. } => "waiver".to_string(),
        }
    }
}

/// One claim that could not be applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedClaim {
    /// Requirement id.
    pub id: String,
    /// Why the claim is unsupported.
    pub reason: String,
}

/// The result of merging and validating an overlay against a profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OverlayOutcome {
    /// Structurally valid claims to execute at the gate.
    pub claims: Vec<Claim>,
    /// Claims rejected before execution.
    pub unsupported: Vec<UnsupportedClaim>,
}

/// A loaded set of overlay rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    rows: Vec<OverlayRow>,
}

impl Overlay {
    /// Load the overlay from the management POSIX directory for `root`.
    ///
    /// # Errors
    ///
    /// Returns an error when the management root is absent or an overlay file is invalid.
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_at(&crate::mgmt::posix_dir(root)?)
    }

    /// Load the optional aggregate file and every `evidence/P1-P5.toml` or
    /// `P<owner>-<batch>.toml` file under an explicit management POSIX directory.
    ///
    /// Duplicate ids and files outside the P1-P5 owner split are hard errors. A missing overlay
    /// is an empty overlay, so the seed and gate keep working before owners add evidence.
    pub fn load_at(posix: &Path) -> Result<Self> {
        let mut paths: Vec<(PathBuf, String)> = Vec::new();
        let aggregate = posix.join("evidence.toml");
        if aggregate.is_file() {
            paths.push((aggregate, String::new()));
        }
        let owner_dir = posix.join("evidence");
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
                let stem = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                // One file per owner, or one per owner sub-batch (`P4-text.toml`); the
                // sub-batch is what lets several minions close their own rows in parallel.
                let owner = if OWNERS.contains(&stem) {
                    stem.to_string()
                } else if let Some((prefix, _)) = stem.split_once('-')
                    && OWNERS.contains(&prefix)
                {
                    prefix.to_string()
                } else {
                    bail!(
                        "overlay file `{}` must be named P1.toml through P5.toml or P<owner>-<batch>.toml",
                        path.display()
                    );
                };
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

    /// Merge structurally valid rows into `profile`, leaving unsupported claims untouched.
    ///
    /// Existence of the qtest case or test path is checked here. Execution is the gate's job; the
    /// seed must not run tests.
    #[must_use]
    pub fn apply(&self, root: &Path, profile: &mut Profile) -> OverlayOutcome {
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
                    let evidence = match parse_evidence(root, &row.evidence) {
                        Ok(evidence) => evidence,
                        Err(reason) => {
                            outcome.unsupported.push(UnsupportedClaim {
                                id: row.id.clone(),
                                reason,
                            });
                            continue;
                        }
                    };
                    let command = evidence.command();
                    apply_present(meta, &command, row.notes.trim());
                    outcome.claims.push(match evidence {
                        Evidence::Qtest(case) => Claim::Qtest {
                            id: row.id.clone(),
                            case,
                        },
                        Evidence::Test(path) => Claim::Test {
                            id: row.id.clone(),
                            path,
                        },
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
                    outcome.claims.push(Claim::Waived {
                        id: row.id.clone(),
                        waiver: waiver.to_string(),
                    });
                }
            }
        }
        outcome
    }
}

fn parse_evidence(root: &Path, value: &str) -> Result<Evidence, String> {
    let value = value.trim();
    if let Some(case) = value.strip_prefix("qtest:") {
        let case = case.trim();
        if case.is_empty() {
            return Err("qtest evidence needs a case name".to_string());
        }
        if case.contains('/') || case.contains('\\') || case == "." || case == ".." {
            return Err(format!("qtest case `{case}` must be a file stem"));
        }
        let path = root.join("tests/qtest/cases").join(format!("{case}.toml"));
        if !path.is_file() {
            return Err(format!(
                "qtest case `{case}` does not exist at {}",
                path.display()
            ));
        }
        return Ok(Evidence::Qtest(case.to_string()));
    }
    if let Some(path) = value.strip_prefix("test:") {
        let path = path.trim();
        if path.is_empty() {
            return Err("test evidence needs a repo-relative path".to_string());
        }
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "test evidence `{path}` must be a repo-relative path without `..`"
            ));
        }
        let absolute = root.join(relative);
        if !absolute.is_file() {
            return Err(format!(
                "test evidence `{path}` does not exist at {}",
                absolute.display()
            ));
        }
        return Ok(Evidence::Test(relative.to_path_buf()));
    }
    Err("evidence must be `qtest:<case>` or `test:<repo-relative-path>`".to_string())
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
