// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Recipe loading and validation (ticket C35, section 4.3).
//!
//! One family is one directory `recipes/<family>/oracle.toml`. The file is strict: unknown keys
//! are errors, the family must match the directory, exports must be sorted and unique, sources must
//! be relative `.c` files, and compiler flags are limited to a hand-written whitelist.
//! `defaults.toml` lives next to the `recipes` directory and holds the shared macro definitions.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Optimisation levels accepted by `opt` and by `--opt`.
pub(crate) const OPT_LEVELS: &[&str] = &["O0", "O1", "O2", "O3", "Os"];

/// One donor source file of a family.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    /// Donor key, as in `donors.lock`.
    pub(crate) donor: String,
    /// Path inside the donor checkout, relative and ending in `.c`.
    pub(crate) path: String,
}

/// One family recipe.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Recipe {
    /// Recipe schema version; must be 1.
    pub(crate) schema: u32,
    /// Family name; must match the directory name.
    pub(crate) family: String,
    /// One-line description.
    pub(crate) description: String,
    /// Source kind: `c` (default) or `header`.
    #[serde(default = "default_kind", rename = "type")]
    pub(crate) kind: String,
    /// Model-only differential: no donor source is compiled.
    #[serde(default)]
    pub(crate) model: bool,
    /// Every defined global of the merged donor object, sorted and unique.
    pub(crate) exports: Vec<String>,
    /// Optimisation levels to build; defaults to `["O2"]`.
    #[serde(default = "default_opt")]
    pub(crate) opt: Vec<String>,
    /// Extra `-D` definitions, added after `defaults.toml`.
    #[serde(default)]
    pub(crate) defines: Vec<String>,
    /// Extra include directories (`donor:<donor>/<dir>` or `shim:<dir>`).
    #[serde(default)]
    pub(crate) include: Vec<String>,
    /// Extra whitelisted compiler flags.
    #[serde(default)]
    pub(crate) flags: Vec<String>,
    /// Donor sources, in link order.
    #[serde(default)]
    pub(crate) source: Vec<Source>,
}

fn default_opt() -> Vec<String> {
    vec!["O2".to_string()]
}

fn default_kind() -> String {
    "c".to_string()
}

/// Shared defaults, read from `<recipes>/../defaults.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Defaults {
    /// Defaults schema version; must be 1.
    pub(crate) schema: u32,
    /// `-D` definitions applied to every recipe.
    #[serde(default)]
    pub(crate) defines: Vec<String>,
}

/// Load and validate one family recipe.
pub(crate) fn load_family(
    recipes: &Path,
    family: &str,
    known_donors: Option<&BTreeSet<String>>,
) -> Result<Recipe> {
    let path = recipes.join(family).join("oracle.toml");
    let recipe = read_recipe(&path)?;
    recipe.validate(family, known_donors)?;
    Ok(recipe)
}

/// Load every family recipe, sorted by family name.
pub(crate) fn load_all(
    recipes: &Path,
    known_donors: Option<&BTreeSet<String>>,
) -> Result<Vec<Recipe>> {
    let entries = fs::read_dir(recipes)
        .with_context(|| format!("cannot read recipes directory {}", recipes.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read {}", recipes.display()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        names.push(name);
    }
    names.sort();
    let mut loaded = Vec::new();
    for name in names {
        loaded.push(load_family(recipes, &name, known_donors)?);
    }
    Ok(loaded)
}

/// Read the shared defaults file next to the recipes directory.
pub(crate) fn read_defaults(recipes: &Path) -> Result<Defaults> {
    let root = recipes.parent().unwrap_or_else(|| Path::new("."));
    let path = root.join("defaults.toml");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read defaults {}", path.display()))?;
    let defaults: Defaults = toml::from_str(&text)
        .with_context(|| format!("defaults {} is not valid TOML", path.display()))?;
    if defaults.schema != 1 {
        bail!(
            "defaults {}: schema {} is not supported (expected 1)",
            path.display(),
            defaults.schema
        );
    }
    for define in &defaults.defines {
        if !valid_define(define) {
            bail!("defaults {}: invalid define `{define}`", path.display());
        }
    }
    Ok(defaults)
}

/// Split and validate a comma-separated `--opt` value.
pub(crate) fn parse_opt_list(value: &str) -> Result<Vec<String>> {
    let mut levels = Vec::new();
    for level in value.split(',') {
        if !OPT_LEVELS.contains(&level) {
            bail!("unknown optimisation level `{level}` (expected one of {OPT_LEVELS:?})");
        }
        if levels.iter().any(|seen| seen == level) {
            bail!("optimisation level `{level}` repeated");
        }
        levels.push(level.to_string());
    }
    if levels.is_empty() {
        bail!("--opt needs at least one level");
    }
    Ok(levels)
}

fn read_recipe(path: &Path) -> Result<Recipe> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("cannot read recipe {}", path.display()))?;
    let recipe: Recipe = toml::from_str(&text)
        .with_context(|| format!("recipe {} is not valid TOML", path.display()))?;
    Ok(recipe)
}

impl Recipe {
    /// Validate the recipe against its directory name and the donor pin set.
    pub(crate) fn validate(
        &self,
        directory: &str,
        known_donors: Option<&BTreeSet<String>>,
    ) -> Result<()> {
        if self.schema != 1 {
            bail!(
                "recipe {}: schema {} is not supported (expected 1)",
                self.family,
                self.schema
            );
        }
        if !valid_family(&self.family) {
            bail!("recipe {}: family must match [a-z0-9-]+", directory);
        }
        if self.family != directory {
            bail!(
                "recipe {}: family `{}` does not match the directory name",
                directory,
                self.family
            );
        }
        if self.description.trim().is_empty() {
            bail!("recipe {}: description is empty", self.family);
        }
        if self.kind != "c" && self.kind != "header" {
            bail!(
                "recipe {}: type `{}` must be `c` or `header`",
                self.family,
                self.kind
            );
        }
        if self.model {
            if !self.source.is_empty() || !self.exports.is_empty() {
                bail!(
                    "recipe {}: a model-only family has no sources or exports",
                    self.family
                );
            }
            return Ok(());
        }
        if self.exports.is_empty() && self.kind != "header" {
            bail!("recipe {}: exports is empty", self.family);
        }
        if self.kind == "header" && !self.exports.is_empty() {
            bail!(
                "recipe {}: a header-only family exports no globals",
                self.family
            );
        }
        for pair in self.exports.windows(2) {
            if pair[0] >= pair[1] {
                bail!(
                    "recipe {}: exports must be sorted and unique (`{}` before `{}`)",
                    self.family,
                    pair[0],
                    pair[1]
                );
            }
        }
        if self.opt.is_empty() {
            bail!("recipe {}: opt is empty", self.family);
        }
        let mut seen = BTreeSet::new();
        for level in &self.opt {
            if !OPT_LEVELS.contains(&level.as_str()) {
                bail!("recipe {}: unknown opt `{level}`", self.family);
            }
            if !seen.insert(level) {
                bail!("recipe {}: opt `{level}` repeated", self.family);
            }
        }
        for define in &self.defines {
            if !valid_define(define) {
                bail!("recipe {}: invalid define `{define}`", self.family);
            }
        }
        for include in &self.include {
            if !valid_include(include) {
                bail!("recipe {}: invalid include `{include}`", self.family);
            }
        }
        for flag in &self.flags {
            if !allowed_flag(flag) {
                bail!("recipe {}: flag `{flag}` is not whitelisted", self.family);
            }
        }
        if self.source.is_empty() {
            bail!(
                "recipe {}: at least one [[source]] is required",
                self.family
            );
        }
        let mut stems = BTreeSet::new();
        for source in &self.source {
            if !valid_source_path(&source.path, &self.kind) {
                let extension = if self.kind == "header" { ".h" } else { ".c" };
                bail!(
                    "recipe {}: source path `{}` must be relative, without `..`, and end in {extension}",
                    self.family,
                    source.path
                );
            }
            if let Some(known) = known_donors
                && !known.contains(&source.donor)
            {
                bail!(
                    "recipe {}: donor `{}` is not in donors.lock",
                    self.family,
                    source.donor
                );
            }
            let stem = Path::new(&source.path)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !stems.insert(stem.clone()) {
                bail!(
                    "recipe {}: two sources share the object stem `{stem}`",
                    self.family
                );
            }
        }
        Ok(())
    }
}

fn valid_family(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_source_path(path: &str, kind: &str) -> bool {
    let extension = if kind == "header" { ".h" } else { ".c" };
    !path.is_empty()
        && path.ends_with(extension)
        && !path.starts_with('/')
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_define(define: &str) -> bool {
    !define.is_empty() && !define.starts_with('-') && !define.chars().any(char::is_whitespace)
}

fn valid_relative_dir(dir: &str) -> bool {
    !dir.is_empty()
        && !dir.starts_with('/')
        && !dir.contains('\\')
        && Path::new(dir)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_include(include: &str) -> bool {
    if let Some(rest) = include.strip_prefix("donor:") {
        let Some((donor, dir)) = rest.split_once('/') else {
            return false;
        };
        !donor.is_empty()
            && donor
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && valid_relative_dir(dir)
    } else if let Some(dir) = include.strip_prefix("shim:") {
        valid_relative_dir(dir)
    } else {
        false
    }
}

/// Whether a compiler flag is on the recipe whitelist of section 4.3.
fn allowed_flag(flag: &str) -> bool {
    let Some(body) = flag.strip_prefix('-') else {
        return false;
    };
    if matches!(
        body,
        "fno-builtin"
            | "fPIC"
            | "fwrapv"
            | "fno-strict-aliasing"
            | "fno-stack-protector"
            | "fno-common"
            | "w"
            | "m64"
    ) {
        return true;
    }
    if let Some(visibility) = body.strip_prefix("fvisibility=") {
        return !visibility.is_empty() && visibility.chars().all(|c| c.is_ascii_lowercase());
    }
    if let Some(std) = body.strip_prefix("std=") {
        return !std.is_empty()
            && std
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '+');
    }
    if let Some(rest) = body.strip_prefix('W') {
        return rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '=' || c == '-');
    }
    false
}

#[cfg(test)]
#[path = "recipe_ut.rs"]
mod tests;
