// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Configuration-level operations on a parsed [`Ruleset`].
//!
//! The parser produces a [`Ruleset`] (filter rules, named tables and the NAT table). This module
//! adds the name-addressed table operations a configuration front-end needs — add, delete and
//! lookup — and the per-rule sticky-address switch, without duplicating the ruleset storage.
//!
//! Donor: FreeBSD `sys/netpfil/pf/pf_table.c` `pfr_match_addr` (lookup), `pfr_add_addrs` (add)
//! and `pfr_del_addrs` (delete, exact); `sys/netpfil/pf/pf_lb.c` `pf_map_addr_sn` for the
//! `PF_POOL_STICKYADDR` option. Table names are matched case-insensitively, as the parser rejects
//! duplicates that way.

use core::fmt;

use crate::address::{AddrSpec, IpAddr};
use crate::ast::Ruleset;
use crate::table::{Table, TableError};

/// Why a configuration-level operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// No table with that name exists in the ruleset.
    UnknownTable,
    /// The table rejected the entry (nested table, full table).
    Table(TableError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTable => f.write_str("unknown table"),
            Self::Table(error) => write!(f, "table: {error}"),
        }
    }
}

impl Ruleset {
    /// The named table, matched case-insensitively.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables
            .iter()
            .find(|table| table.name.eq_ignore_ascii_case(name))
    }

    /// The named table for mutation.
    #[must_use]
    pub fn table_mut(&mut self, name: &str) -> Option<&mut Table> {
        self.tables
            .iter_mut()
            .find(|table| table.name.eq_ignore_ascii_case(name))
    }

    /// True when `address` is covered by the named table (donor `pfr_match_addr`).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownTable`] when the name is not defined.
    pub fn table_contains(&self, name: &str, address: IpAddr) -> Result<bool, ConfigError> {
        let table = self.table(name).ok_or(ConfigError::UnknownTable)?;
        Ok(table.lookup(address, &self.tables))
    }

    /// Append one entry to the named table (donor `pfr_add_addrs` for one address).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownTable`] for an undefined table, or
    /// [`ConfigError::Table`] when the table rejects the entry.
    pub fn add_table_entry(&mut self, name: &str, spec: AddrSpec) -> Result<(), ConfigError> {
        let table = self
            .tables
            .iter_mut()
            .find(|table| table.name.eq_ignore_ascii_case(name))
            .ok_or(ConfigError::UnknownTable)?;
        table.add(spec).map_err(ConfigError::Table)
    }

    /// Delete one exact entry from the named table (donor `pfr_del_addrs`, exact).
    ///
    /// Returns `Ok(true)` when the entry was present. A covering entry is not removed.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownTable`] when the name is not defined.
    pub fn delete_table_entry(&mut self, name: &str, spec: &AddrSpec) -> Result<bool, ConfigError> {
        let table = self.table_mut(name).ok_or(ConfigError::UnknownTable)?;
        Ok(table.delete(spec))
    }

    /// Set the sticky-address option for NAT rule `index` (donor `PF_POOL_STICKYADDR`).
    ///
    /// Returns `false` for an out-of-range rule index.
    pub fn set_nat_sticky(&mut self, index: usize, sticky: bool) -> bool {
        self.nat.set_sticky(index, sticky)
    }

    /// The sticky-address option for NAT rule `index` (`None` when out of range).
    #[must_use]
    pub fn nat_sticky(&self, index: usize) -> Option<bool> {
        self.nat.sticky(index)
    }
}
