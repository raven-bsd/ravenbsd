// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-file donor attribution for generated material.
//!
//! A constant table is musl-derived unless its sources carry an explicit `<donor>:<path>`
//! prefix and the table carries the matching donor lease. The renderer never falls back to the
//! musl notice for a non-musl table: an inconsistent or missing lease is a hard error.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::model::ConstFile;
use crate::provenance::{CommentStyle, Lock, musl_notice};
use crate::validate::Tables;

/// The reviewed source of one generated group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    /// Pinned donor name.
    pub donor: String,
    /// Donor-relative source paths.
    pub paths: Vec<String>,
    /// Exact donor revision read from `donors.lock`.
    pub revision: String,
    /// Per-file licence of the reviewed source.
    pub license: String,
}

impl Attribution {
    /// The notice block for one generated file.
    #[must_use]
    pub fn notice(&self, style: CommentStyle) -> String {
        if self.donor == "musl" {
            return musl_notice(style, &self.paths.join(", "), &self.revision);
        }
        let lines = [
            format!("SPDX-License-Identifier: {}", self.license),
            "Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors".to_string(),
            String::new(),
            format!(
                "RavenBSD: derived from {}:{} @ {}",
                self.donor,
                self.paths.join(", "),
                self.revision
            ),
            "(reviewed source; constants transcribed to TOML and generated, not copied)"
                .to_string(),
            String::new(),
        ];
        let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
        style_wrap(style, &borrowed)
    }
}

fn style_wrap(style: CommentStyle, lines: &[&str]) -> String {
    match style {
        CommentStyle::LineSlash => lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    "//".to_string()
                } else {
                    format!("// {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CommentStyle::LineHash => lines
            .iter()
            .map(|line| {
                if line.is_empty() {
                    "#".to_string()
                } else {
                    format!("# {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CommentStyle::BlockC => {
            let mut out = String::from("/*\n");
            for line in lines {
                if line.is_empty() {
                    out.push_str(" *\n");
                } else {
                    out.push_str(" * ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out.push_str(" */");
            out
        }
    }
}

/// Every group's attribution, keyed by file stem.
#[derive(Debug, Clone)]
pub struct Attributions {
    by_stem: BTreeMap<String, Attribution>,
}

impl Attributions {
    /// Resolve every table's donor and reject an inconsistent lease.
    pub fn resolve(tables: &Tables, lock: &Lock) -> Result<Self> {
        let mut by_stem = BTreeMap::new();
        for group in &tables.groups {
            let attribution = resolve_group(&group.def, lock)?;
            by_stem.insert(group.stem.clone(), attribution);
        }
        Ok(Self { by_stem })
    }

    /// One group's attribution.
    #[must_use]
    pub fn get(&self, stem: &str) -> Option<&Attribution> {
        self.by_stem.get(stem)
    }
}

/// Split `<donor>:<path>`; a bare path is musl-relative.
fn split_source(source: &str) -> Result<(&str, &str)> {
    match source.split_once(':') {
        Some((prefix, path)) if !prefix.is_empty() && !prefix.contains('/') => {
            if lock_known(prefix) {
                Ok((prefix, path))
            } else {
                bail!("unknown donor prefix {prefix:?} in source {source:?}")
            }
        }
        _ => Ok(("musl", source)),
    }
}

fn lock_known(donor: &str) -> bool {
    matches!(donor, "musl" | "freebsd" | "linux-raw-sys")
}

/// Resolve one group and reject a donor table that claims musl-only provenance.
pub fn resolve_group(def: &ConstFile, lock: &Lock) -> Result<Attribution> {
    let mut donors = BTreeMap::new();
    for source in &def.source {
        let (donor, path) = split_source(source)?;
        if path.is_empty() {
            bail!("group {} has an empty source path", def.group);
        }
        donors
            .entry(donor.to_string())
            .or_insert_with(Vec::new)
            .push(path.to_string());
    }
    if donors.len() > 1 {
        bail!(
            "group {} mixes donor sources {:?}; one table has one donor",
            def.group,
            donors.keys().collect::<Vec<_>>()
        );
    }
    let (donor, paths) = donors
        .into_iter()
        .next()
        .unwrap_or_else(|| ("musl".to_string(), Vec::new()));

    if let Some(spec) = &def.donor
        && spec.name != "musl"
        && paths.is_empty()
    {
        bail!(
            "group {} leases {} but lists no {}:<path> source",
            def.group,
            spec.name,
            spec.name
        );
    }

    match &def.donor {
        None => {
            if donor != "musl" {
                bail!(
                    "group {} is derived from {donor} but has no donor lease; \
                     refusing to label it musl-derived",
                    def.group
                );
            }
            Ok(Attribution {
                donor,
                paths,
                revision: lock.musl.clone(),
                license: "MIT".to_string(),
            })
        }
        Some(spec) => {
            if spec.name != donor {
                bail!(
                    "group {} names donor {:?} in its lease but its sources resolve to {:?}; \
                     a FreeBSD-derived table cannot be attributed to musl",
                    def.group,
                    spec.name,
                    donor
                );
            }
            if donor == "musl" {
                if spec.revision != lock.musl {
                    bail!(
                        "group {} leases musl {} but donors.lock pins {}",
                        def.group,
                        spec.revision,
                        lock.musl
                    );
                }
                return Ok(Attribution {
                    donor,
                    paths,
                    revision: lock.musl.clone(),
                    license: "MIT".to_string(),
                });
            }
            if paths.is_empty() {
                bail!(
                    "group {} leases {donor} but lists no {donor}:<path> source",
                    def.group
                );
            }
            let pinned = lock
                .revision_of(&donor)
                .ok_or_else(|| anyhow::anyhow!("donors.lock does not pin donor {donor:?}"))?;
            if spec.revision != pinned {
                bail!(
                    "group {} leases {donor} {} but donors.lock pins {pinned}",
                    def.group,
                    spec.revision
                );
            }
            if !licence_allowed(&spec.license) {
                bail!(
                    "group {} declares licence {:?} which is outside the donor allow-list",
                    def.group,
                    spec.license
                );
            }
            Ok(Attribution {
                donor,
                paths,
                revision: spec.revision.clone(),
                license: spec.license.clone(),
            })
        }
    }
}

fn licence_allowed(license: &str) -> bool {
    matches!(
        license,
        "MIT" | "BSD-2-Clause" | "BSD-3-Clause" | "ISC" | "Apache-2.0" | "0BSD" | "Zlib"
    )
}

#[cfg(test)]
#[path = "attribution_ut.rs"]
mod tests;
