// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `world.toml` schema, validation, subsets and build order (section 4.2).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::model::{BUILDERS, IMPLEMENTED_BUILDERS};

/// The placeholder word used until the H-12 freeze.
pub const PLACEHOLDER: &str = "PLACEHOLDER";

/// Optional-package weight table.
#[derive(Clone, Debug, Deserialize)]
pub struct Weights {
    /// Weight of an optional row.
    #[serde(default = "default_optional_weight")]
    pub optional: f64,
}

fn default_optional_weight() -> f64 {
    0.5
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            optional: default_optional_weight(),
        }
    }
}

/// One package row.
#[derive(Clone, Debug, Deserialize)]
pub struct Package {
    /// Row number in the 200-row list.
    pub n: u32,
    /// Package name.
    pub name: String,
    /// Build order tier.
    pub tier: u8,
    /// Build key (`ac`, `cm`, `me`, `mk`, `ca`, `go`, `py`, `cf`, `kc`, `b2`).
    pub build: String,
    /// Version, or `PLACEHOLDER`.
    pub version: String,
    /// Upstream URL, or `PLACEHOLDER`.
    pub url: String,
    /// SHA-256 of the tarball, or `PLACEHOLDER`.
    pub sha256: String,
    /// Tarball file name, or `PLACEHOLDER`.
    pub tarball: String,
    /// Package dependencies.
    #[serde(default)]
    pub needs: Vec<String>,
    /// Optional row (weight 0.5).
    #[serde(default)]
    pub optional: bool,
    /// Why the package stresses libc (verbatim from the specification table).
    pub stresses: String,
    /// Extra `configure` / `cmake` flags.
    #[serde(default)]
    pub configure: Vec<String>,
    /// Reason for every disabled feature or cached probe.
    #[serde(default)]
    pub flag_notes: BTreeMap<String, String>,
    /// Extra `make` arguments.
    #[serde(default)]
    pub make_args: Vec<String>,
    /// Parallel make width inside the package.
    #[serde(default = "default_jobs")]
    pub jobs_make: u32,
    /// `none` or `make-check`; recorded, never run by this ticket.
    #[serde(default = "default_tests")]
    pub tests: String,
    /// `mk`/`cf` install step.
    #[serde(default)]
    pub install: bool,
    /// `kc` configuration target.
    #[serde(default = "default_config_target")]
    pub config_target: String,
    /// `kc` configuration lines appended before `oldconfig`.
    #[serde(default)]
    pub config_lines: Vec<String>,
}

fn default_jobs() -> u32 {
    4
}

fn default_tests() -> String {
    "none".to_string()
}

fn default_config_target() -> String {
    "defconfig".to_string()
}

/// The parsed world list.
#[derive(Clone, Debug, Deserialize)]
pub struct World {
    /// Schema version.
    pub schema: u32,
    /// `pending` or `frozen`.
    pub freeze: String,
    /// Weight table.
    #[serde(default)]
    pub weights: Weights,
    /// Named package subsets.
    #[serde(default)]
    pub subset: BTreeMap<String, Vec<String>>,
    /// The 200 package rows.
    #[serde(default)]
    pub package: Vec<Package>,
}

impl World {
    /// Load and parse a world file (validation is separate).
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read world list {}", path.display()))?;
        let world: Self = toml::from_str(&text)
            .with_context(|| format!("cannot parse world list {}", path.display()))?;
        Ok(world)
    }

    /// Parse from text (tests).
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// One package by name.
    #[must_use]
    pub fn package(&self, name: &str) -> Option<&Package> {
        self.package.iter().find(|package| package.name == name)
    }

    /// All rows in file order.
    #[must_use]
    pub fn packages(&self) -> &[Package] {
        &self.package
    }

    /// The weighted total of section 3.B.6.2.
    #[must_use]
    pub fn weighted_total(&self) -> f64 {
        self.package
            .iter()
            .map(|package| {
                if package.optional {
                    self.weights.optional
                } else {
                    1.0
                }
            })
            .sum()
    }

    /// Whether the world is frozen.
    #[must_use]
    pub fn is_frozen(&self) -> bool {
        self.freeze == "frozen"
    }

    /// Validate every rule of section 4.2 for the full 200-row list.
    pub fn validate(&self) -> Result<()> {
        self.validate_with(200, 197.5)
    }

    /// Validate the rules against a list of `expected_rows` rows and `expected_weight` total.
    /// The fixture tests use a small list; the shipped file always uses 200 / 197.5.
    pub fn validate_with(&self, expected_rows: usize, expected_weight: f64) -> Result<()> {
        self.validate_structure(expected_rows, expected_weight)?;
        self.check_placeholders()
    }

    /// Validate everything except the freeze/placeholder discipline.
    ///
    /// `build` uses this because the real-subset procedure in the ticket puts real values in a
    /// few rows of a scratch copy while the other rows keep their placeholders.
    pub fn validate_structure(&self, expected_rows: usize, expected_weight: f64) -> Result<()> {
        if self.schema != 1 {
            bail!("schema must be 1, found {}", self.schema);
        }
        if self.freeze != "pending" && self.freeze != "frozen" {
            bail!(
                "freeze must be `pending` or `frozen`, found `{}`",
                self.freeze
            );
        }
        if self.package.len() != expected_rows {
            bail!(
                "the list must have {expected_rows} package rows, found {}",
                self.package.len()
            );
        }
        let mut numbers = BTreeSet::new();
        for package in &self.package {
            if !(1..=expected_rows as u32).contains(&package.n) {
                bail!(
                    "package `{}` has row number {} outside 1..={expected_rows}",
                    package.name,
                    package.n
                );
            }
            if !numbers.insert(package.n) {
                bail!("row number {} is repeated", package.n);
            }
        }
        for number in 1..=expected_rows as u32 {
            if !numbers.contains(&number) {
                bail!("row number {number} is missing");
            }
        }
        let mut names = BTreeSet::new();
        for package in &self.package {
            if !valid_name(&package.name) {
                bail!(
                    "package name `{}` is not [a-z0-9][a-z0-9+_.-]*",
                    package.name
                );
            }
            if !names.insert(package.name.as_str()) {
                bail!("package name `{}` is repeated", package.name);
            }
        }
        let by_name: BTreeMap<&str, &Package> = self
            .package
            .iter()
            .map(|package| (package.name.as_str(), package))
            .collect();
        for package in &self.package {
            if expected_rows == 200 {
                let expected = expected_tier(package.n);
                if package.tier != expected {
                    bail!(
                        "package `{}` (row {}) is tier {}, the list says {}",
                        package.name,
                        package.n,
                        package.tier,
                        expected
                    );
                }
            }
            if !BUILDERS.contains(&package.build.as_str()) {
                bail!(
                    "package `{}` has unknown build key `{}`",
                    package.name,
                    package.build
                );
            }
            if !IMPLEMENTED_BUILDERS.contains(&package.build.as_str())
                && !matches!(package.build.as_str(), "me" | "ca" | "go" | "py" | "b2")
            {
                bail!(
                    "package `{}` has unknown build key `{}`",
                    package.name,
                    package.build
                );
            }
            if package.stresses.trim().is_empty() {
                bail!("package `{}` has an empty `stresses` text", package.name);
            }
            if package.jobs_make == 0 {
                bail!("package `{}` has jobs_make = 0", package.name);
            }
            if package.tests != "none" && package.tests != "make-check" {
                bail!(
                    "package `{}` has unknown tests value `{}`",
                    package.name,
                    package.tests
                );
            }
            if package.config_target != "defconfig" && package.config_target != "allnoconfig" {
                bail!(
                    "package `{}` has unknown config_target `{}`",
                    package.name,
                    package.config_target
                );
            }
            for flag in &package.configure {
                let lower = flag.to_ascii_lowercase();
                if lower.contains("libc") || lower.contains("musl") || lower.contains("glibc") {
                    bail!(
                        "package `{}`: configure flag `{flag}` names a libc; a libc-triggered disable is forbidden",
                        package.name
                    );
                }
                let needs_note = flag.starts_with("--disable-")
                    || flag.starts_with("--without-")
                    || flag.starts_with("ac_cv_");
                if needs_note {
                    let Some(note) = package.flag_notes.get(flag) else {
                        bail!(
                            "package `{}`: `{flag}` needs a `flag_notes` entry",
                            package.name
                        );
                    };
                    if !note.starts_with("platform:") && !note.starts_with("libc:") {
                        bail!(
                            "package `{}`: note for `{flag}` must start with `platform:` or `libc:`",
                            package.name
                        );
                    }
                }
            }
            for (flag, note) in &package.flag_notes {
                if note.trim().is_empty() {
                    bail!("package `{}`: empty note for `{flag}`", package.name);
                }
                if !note.starts_with("platform:") && !note.starts_with("libc:") {
                    bail!(
                        "package `{}`: note for `{flag}` must start with `platform:` or `libc:`",
                        package.name
                    );
                }
            }
            for need in &package.needs {
                if !by_name.contains_key(need.as_str()) {
                    bail!("package `{}` needs unknown package `{need}`", package.name);
                }
                if need == &package.name {
                    bail!("package `{}` needs itself", package.name);
                }
            }
        }
        let total = self.weighted_total();
        if (total - expected_weight).abs() > 1e-9 {
            bail!("weighted total is {total}, expected {expected_weight}");
        }
        for subset in self.subset.keys() {
            self.subset_names(subset)?;
        }
        self.check_acyclic()?;
        Ok(())
    }

    fn check_placeholders(&self) -> Result<()> {
        for package in &self.package {
            let fields = [
                ("version", &package.version),
                ("url", &package.url),
                ("sha256", &package.sha256),
                ("tarball", &package.tarball),
            ];
            if self.freeze == "pending" {
                for (key, value) in fields {
                    if value != PLACEHOLDER {
                        bail!(
                            "freeze is pending but `{}` has a real {key} `{value}`; versions rot before H-12",
                            package.name
                        );
                    }
                }
            } else {
                for (key, value) in fields {
                    if value.is_empty() || value == PLACEHOLDER {
                        bail!("world is frozen but `{}` has no {key}", package.name);
                    }
                }
                if package.sha256.len() != 64
                    || !package
                        .sha256
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                {
                    bail!("package `{}` has an invalid sha256", package.name);
                }
            }
        }
        Ok(())
    }

    fn check_acyclic(&self) -> Result<()> {
        let mut state: BTreeMap<&str, u8> = BTreeMap::new();
        let index: BTreeMap<&str, usize> = self
            .package
            .iter()
            .enumerate()
            .map(|(position, package)| (package.name.as_str(), position))
            .collect();
        for package in &self.package {
            self.visit(package, &index, &mut state)?;
        }
        Ok(())
    }

    fn visit<'a>(
        &'a self,
        package: &'a Package,
        index: &BTreeMap<&'a str, usize>,
        state: &mut BTreeMap<&'a str, u8>,
    ) -> Result<()> {
        match state.get(package.name.as_str()).copied() {
            Some(2) => return Ok(()),
            Some(1) => bail!("needs cycle through `{}`", package.name),
            _ => {}
        }
        state.insert(package.name.as_str(), 1);
        for need in &package.needs {
            let position = index
                .get(need.as_str())
                .ok_or_else(|| anyhow::anyhow!("unknown package `{need}`"))?;
            let target = &self.package[*position];
            self.visit(target, index, state)?;
        }
        state.insert(package.name.as_str(), 2);
        Ok(())
    }

    /// The names of one subset, in name order, or a tier subset.
    pub fn subset_names(&self, name: &str) -> Result<Vec<String>> {
        if let Some(names) = self.subset.get(name) {
            for package in names {
                if self.package(package).is_none() {
                    bail!("subset `{name}` names unknown package `{package}`");
                }
            }
            return Ok(names.clone());
        }
        if let Some(tier) = name
            .strip_prefix("tier")
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|tier| (1..=5).contains(tier))
        {
            return Ok(self
                .package
                .iter()
                .filter(|package| package.tier == tier)
                .map(|package| package.name.clone())
                .collect());
        }
        bail!("unknown subset `{name}`");
    }

    /// Select rows for `--all`, `--tier`, `--subset` or explicit names.
    pub fn select(
        &self,
        names: &[String],
        tier: Option<u8>,
        subset: Option<&str>,
    ) -> Result<Vec<&Package>> {
        let mut selected: Vec<&Package> = Vec::new();
        if !names.is_empty() {
            for name in names {
                let package = self
                    .package(name)
                    .ok_or_else(|| anyhow::anyhow!("unknown package `{name}`"))?;
                selected.push(package);
            }
        }
        if let Some(tier) = tier {
            if !(1..=5).contains(&tier) {
                bail!("tier must be 1..=5, found {tier}");
            }
            selected.extend(self.package.iter().filter(|package| package.tier == tier));
        }
        if let Some(subset) = subset {
            for name in self.subset_names(subset)? {
                let package = self
                    .package(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown package `{name}`"))?;
                selected.push(package);
            }
        }
        if selected.is_empty() {
            bail!("no packages selected (give names, --subset, --tier or --all)");
        }
        let mut seen = BTreeSet::new();
        selected.retain(|package| seen.insert(package.name.as_str()));
        self.build_order(&selected)
    }

    /// Topological order over `needs`, ties by `(tier, n)`.
    pub fn build_order<'a>(&self, selected: &[&'a Package]) -> Result<Vec<&'a Package>> {
        let names: BTreeSet<&str> = selected
            .iter()
            .map(|package| package.name.as_str())
            .collect();
        let mut pending: Vec<&Package> = selected.to_vec();
        let mut order: Vec<&Package> = Vec::with_capacity(pending.len());
        while !pending.is_empty() {
            let mut ready: Vec<usize> = pending
                .iter()
                .enumerate()
                .filter(|(_, package)| {
                    package
                        .needs
                        .iter()
                        .filter(|need| names.contains(need.as_str()))
                        .all(|need| order.iter().any(|done| &done.name == need))
                })
                .map(|(index, _)| index)
                .collect();
            if ready.is_empty() {
                bail!(
                    "needs cycle or unresolved dependency among: {}",
                    pending
                        .iter()
                        .map(|package| package.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            ready.sort_by_key(|index| (pending[*index].tier, pending[*index].n));
            let chosen = pending.remove(ready[0]);
            order.push(chosen);
        }
        Ok(order)
    }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    chars
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '_' | '.' | '-'))
}

/// The tier the specification assigns to a row number.
#[must_use]
pub fn expected_tier(n: u32) -> u8 {
    match n {
        22..=47 | 196 => 1,
        81 | 82 | 145..=147 | 180..=185 => 5,
        74..=100 => 2,
        101..=133 => 3,
        1..=21 | 48..=73 | 134..=179 | 186..=200 => 4,
        _ => 4,
    }
}

#[cfg(test)]
#[path = "world_ut.rs"]
mod tests;
