// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Runtime `pf.conf` configuration model.
//!
//! [`Conf`] owns the parsed [`Ruleset`] and exposes the table operations `pfctl -T add`,
//! `pfctl -T delete` and a rule lookup need after the file has been loaded. The file parser is
//! [`crate::parser::parse`]; this module does not re-parse or duplicate it.
//!
//! Donor: FreeBSD `sys/netpfil/pf/pf_table.c` (`pfr_lookup_addr`, `pfr_table_add`,
//! `pfr_table_del`) and `usr.sbin/pfctl` table operations (BSD-2-Clause). Raven's table is a
//! bounded ordered value list, so add/delete are a value-list operation rather than a radix
//! tree insert/remove.

use core::fmt;

use crate::address::{AddrSpec, IpAddr};
use crate::ast::Ruleset;
use crate::parser::{ParseError, parse};
use crate::table::TableError;

/// Why a runtime table operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfError {
    /// The table name is not declared in the loaded ruleset.
    UnknownTable,
    /// The table rejected the entry.
    Table(TableError),
}

impl fmt::Display for ConfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTable => f.write_str("unknown table"),
            Self::Table(error) => write!(f, "table: {error}"),
        }
    }
}

/// One loaded `pf.conf` configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conf {
    /// Parsed filter rules, tables and NAT/RDR rules.
    pub ruleset: Ruleset,
}

impl Conf {
    /// Parse a `pf.conf` text into a configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] for the first invalid line.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        parse(text).map(|ruleset| Self { ruleset })
    }

    fn table_index(&self, name: &str) -> Option<usize> {
        self.ruleset
            .tables
            .iter()
            .position(|table| table.name.eq_ignore_ascii_case(name))
    }

    /// Add one address specification to table `name`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfError::UnknownTable`] for an undeclared table.
    pub fn table_add(&mut self, name: &str, spec: AddrSpec) -> Result<(), ConfError> {
        let index = self.table_index(name).ok_or(ConfError::UnknownTable)?;
        self.ruleset
            .tables
            .get_mut(index)
            .ok_or(ConfError::UnknownTable)?
            .add(spec)
            .map_err(ConfError::from)
    }

    /// Delete every entry equal to `spec` from table `name`, returning whether one matched.
    ///
    /// # Errors
    ///
    /// Returns [`ConfError::UnknownTable`] for an undeclared table.
    pub fn table_delete(&mut self, name: &str, spec: &AddrSpec) -> Result<bool, ConfError> {
        let index = self.table_index(name).ok_or(ConfError::UnknownTable)?;
        Ok(self
            .ruleset
            .tables
            .get_mut(index)
            .ok_or(ConfError::UnknownTable)?
            .remove(spec))
    }

    /// True when `address` matches an entry of table `name`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfError::UnknownTable`] for an undeclared table.
    pub fn table_lookup(&self, name: &str, address: IpAddr) -> Result<bool, ConfError> {
        let index = self.table_index(name).ok_or(ConfError::UnknownTable)?;
        let table = self
            .ruleset
            .tables
            .get(index)
            .ok_or(ConfError::UnknownTable)?;
        Ok(table.matches(address, &self.ruleset.tables))
    }
}

impl From<TableError> for ConfError {
    fn from(error: TableError) -> Self {
        Self::Table(error)
    }
}

#[cfg(test)]
#[path = "conf_ut.rs"]
mod tests;
