// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The symbol universe: the union of donor symbol lists, the glibc manual,
//! `--names` and `assignments.toml`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::Kind;

/// One universe member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniverseEntry {
    /// Symbol name.
    pub name: String,
    /// Best-known kind.
    pub kind: Kind,
}

/// A sorted, de-duplicated symbol universe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Universe {
    /// Entries sorted by name.
    pub entries: Vec<UniverseEntry>,
}

impl Universe {
    /// Build from `(name, kind)` pairs. Later data/macro kinds win over `func`
    /// so the variable regex is used when any source marks a name as data.
    pub fn build<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (String, Kind)>,
    {
        let mut kinds: BTreeMap<String, Kind> = BTreeMap::new();
        for (name, kind) in pairs {
            if !is_identifier(&name) {
                continue;
            }
            let entry = kinds.entry(name).or_insert(kind);
            if kind_rank(kind) > kind_rank(*entry) {
                *entry = kind;
            }
        }
        Self {
            entries: kinds
                .into_iter()
                .map(|(name, kind)| UniverseEntry { name, kind })
                .collect(),
        }
    }

    /// Merge another universe into this one.
    pub fn extend(&mut self, other: Universe) {
        let mut pairs: Vec<(String, Kind)> = self
            .entries
            .drain(..)
            .map(|entry| (entry.name, entry.kind))
            .collect();
        pairs.extend(
            other
                .entries
                .into_iter()
                .map(|entry| (entry.name, entry.kind)),
        );
        *self = Self::build(pairs);
    }

    /// Names only, sorted.
    pub fn names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Look up the kind of a name.
    pub fn kind(&self, name: &str) -> Option<Kind> {
        self.entries
            .binary_search_by(|entry| entry.name.as_str().cmp(name))
            .ok()
            .map(|index| self.entries[index].kind)
    }

    /// Whether the universe is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn kind_rank(kind: Kind) -> u8 {
    match kind {
        Kind::Data => 4,
        Kind::Macro => 3,
        Kind::Func => 2,
        Kind::HeaderOnly => 1,
    }
}

/// Whether `name` is an ASCII C identifier of length at least two.
pub fn is_identifier(name: &str) -> bool {
    if name.len() < 2 {
        return false;
    }
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Read a one-name-per-line file (`--names`).
pub fn read_names_file(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| is_identifier(line))
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
#[path = "universe_ut.rs"]
mod tests;
