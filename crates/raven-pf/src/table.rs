// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Named address tables (`table <name> { ... }`).
//!
//! Donor: `sys/netpfil/pf/pf_table.c` (`pfr_match_addr`, `pfr_table_add`) and the
//! `PF_ADDR_TABLE` address kind in `sys/netpfil/pf/pf.h`. A table holds address
//! specifications; a rule refers to it by index, so the rule stays `Copy`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::address::{AddrSpec, IpAddr};

/// Largest number of tables in one ruleset.
pub const MAX_TABLES: usize = 64;
/// Largest number of entries in one table.
pub const MAX_TABLE_ENTRIES: usize = 1024;
/// Largest table nesting depth accepted while matching.
const MAX_MATCH_DEPTH: u8 = 8;

/// Why a table entry was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    /// The table already holds [`MAX_TABLE_ENTRIES`] entries.
    Full,
    /// A table cannot contain another table (`PF_ADDR_TABLE` in a table is refused).
    Nested,
    /// The ruleset already holds [`MAX_TABLES`] tables.
    TooManyTables,
}

impl core::fmt::Display for TableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Full => "table is full",
            Self::Nested => "a table cannot contain another table",
            Self::TooManyTables => "too many tables",
        })
    }
}

/// One named address table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// Table name without the angle brackets.
    pub name: String,
    /// Entries in insertion order.
    pub entries: Vec<AddrSpec>,
}

impl Table {
    /// An empty table.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Vec::new(),
        }
    }

    /// Append one entry, rejecting nested tables and overflow.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Nested`] for an [`AddrSpec::Table`] entry and
    /// [`TableError::Full`] at [`MAX_TABLE_ENTRIES`].
    pub fn add(&mut self, spec: AddrSpec) -> Result<(), TableError> {
        if matches!(spec, AddrSpec::Table(_)) {
            return Err(TableError::Nested);
        }
        if self.entries.len() >= MAX_TABLE_ENTRIES {
            return Err(TableError::Full);
        }
        self.entries.push(spec);
        Ok(())
    }

    /// Replace every entry; each one is validated like [`Table::add`].
    ///
    /// # Errors
    ///
    /// Returns the first [`TableError`] from [`Table::add`].
    pub fn replace(&mut self, entries: Vec<AddrSpec>) -> Result<(), TableError> {
        let mut next = Self::new(self.name.clone());
        for entry in entries {
            next.add(entry)?;
        }
        self.entries = next.entries;
        Ok(())
    }

    /// True when `address` is in the table. Table entries never nest, but the lookup keeps a
    /// depth guard so a corrupted ruleset cannot recurse without bound.
    #[must_use]
    pub fn matches(&self, address: IpAddr, tables: &[Table]) -> bool {
        self.matches_at(address, tables, 0)
    }

    pub(crate) fn matches_at(&self, address: IpAddr, tables: &[Table], depth: u8) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.matches_depth(address, tables, depth))
    }
}

/// Bound a table's entry count as a ruleset is loaded.
///
/// # Errors
///
/// Returns [`TableError::TooManyTables`] once `tables` holds [`MAX_TABLES`].
pub fn check_table_capacity(tables: &[Table]) -> Result<(), TableError> {
    if tables.len() >= MAX_TABLES {
        return Err(TableError::TooManyTables);
    }
    Ok(())
}

/// The nesting guard value, exposed for the address module.
pub(crate) const MATCH_DEPTH_LIMIT: u8 = MAX_MATCH_DEPTH;

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;
