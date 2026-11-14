// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generic instantiation counting (`R-10`): `libc-size monomorph --lib FILE`.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::measure::parse_nm_sizes;

/// The PROPOSED ceiling per generic prefix.
pub const MAX_INSTANTIATIONS: usize = 2;

/// One group of demangled symbols sharing the path before the first `<`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The prefix (or the whole name for a non-generic symbol).
    pub prefix: String,
    /// How often it occurs.
    pub count: usize,
    /// Up to five examples.
    pub examples: Vec<String>,
}

/// Group demangled `llvm-nm --size-sort --demangle` lines.
#[must_use]
pub fn groups(nm_text: &str) -> Vec<Group> {
    let symbols = parse_nm_sizes(nm_text).unwrap_or_default();
    let mut map: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for symbol in symbols {
        let normalized = symbol.name.strip_prefix('<').unwrap_or(&symbol.name);
        let prefix = normalized.split_once('<').map_or_else(
            || normalized.to_string(),
            |(prefix, _)| prefix.trim().to_string(),
        );
        let entry = map.entry(prefix).or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < 5 {
            entry.1.push(symbol.name);
        }
    }
    let mut groups: Vec<Group> = map
        .into_iter()
        .map(|(prefix, (count, examples))| Group {
            prefix,
            count,
            examples,
        })
        .collect();
    groups.sort_by(|a, b| b.count.cmp(&a.count).then(a.prefix.cmp(&b.prefix)));
    groups
}

/// One allow-list row for a generic prefix.
#[derive(Clone, Debug, Deserialize)]
pub struct MonomorphAllow {
    /// The prefix before `<`.
    pub prefix: String,
    /// Why more than two instantiations are accepted.
    #[serde(default)]
    pub reason: String,
}

/// The `[monomorph]` part of `size-allowlist.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AllowFile {
    /// The allowed prefixes.
    #[serde(default, rename = "monomorph")]
    pub monomorph: Vec<MonomorphAllow>,
}

/// Load the allow-list; a missing file is an empty list.
///
/// # Errors
/// A parse error.
pub fn load_allowlist(path: &Path) -> Result<AllowFile> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(AllowFile::default());
    };
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// The groups above the ceiling and not allow-listed.
#[must_use]
pub fn failures<'a>(groups: &'a [Group], allow: &AllowFile) -> Vec<&'a Group> {
    groups
        .iter()
        .filter(|group| group.count > MAX_INSTANTIATIONS)
        .filter(|group| !allow.monomorph.iter().any(|row| row.prefix == group.prefix))
        .collect()
}

/// The top `limit` groups, largest first.
#[must_use]
pub fn top(groups: &[Group], limit: usize) -> Vec<Group> {
    groups.iter().take(limit).cloned().collect()
}

/// Render the report.
#[must_use]
pub fn render(groups: &[Group], allow: &AllowFile) -> String {
    let mut out = String::new();
    for group in top(groups, 20) {
        let mark = if group.count > MAX_INSTANTIATIONS
            && !allow.monomorph.iter().any(|row| row.prefix == group.prefix)
        {
            "FAIL"
        } else {
            "ok"
        };
        out.push_str(&format!("{mark} {:>3} {}", group.count, group.prefix));
        if let Some(example) = group.examples.first() {
            out.push_str(&format!("  [{example}]"));
        }
        out.push('\n');
    }
    let failures = failures(groups, allow);
    out.push_str(&format!(
        "libc-size monomorph: {} groups, {} above {} and not allow-listed\n",
        groups.len(),
        failures.len(),
        MAX_INSTANTIATIONS
    ));
    out
}

#[cfg(test)]
#[path = "monomorph_ut.rs"]
mod tests;
