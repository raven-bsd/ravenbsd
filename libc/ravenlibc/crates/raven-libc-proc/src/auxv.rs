// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The auxiliary-vector table.

use alloc::vec::Vec;

/// The parsed auxiliary vector, preserving the first occurrence of each key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuxTable {
    entries: Vec<(usize, usize)>,
}

impl AuxTable {
    /// Create an empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Append a pair unless the key is already present.
    pub fn insert(&mut self, key: usize, value: usize) {
        if self.get(key).is_none() {
            self.entries.push((key, value));
        }
    }

    /// Look up the first value for `key`.
    #[must_use]
    pub fn get(&self, key: usize) -> Option<usize> {
        self.entries
            .iter()
            .find(|(entry_key, _)| *entry_key == key)
            .map(|(_, value)| *value)
    }

    /// Iterate over the pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.entries.iter().copied()
    }

    /// Number of stored pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no pair is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "auxv_ut.rs"]
mod tests;
