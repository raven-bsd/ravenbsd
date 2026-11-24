// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/etc/shadow` records.
//!
//! The shape matches `raven_auth::ShadowEntry`; this crate keeps its own parser so the user
//! database does not link the hashing code. A unit test compares both parsers on the canonical
//! fixture.

use alloc::format;
use alloc::string::{String, ToString};

use crate::error::{DbError, DbFile};
use crate::validate::{fields, integer, valid_hash, valid_name};

/// Number of colon-separated fields in a shadow record.
pub const FIELDS: usize = 9;

/// One `/etc/shadow` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowEntry {
    /// Account name.
    pub name: String,
    /// `$6$...` hash, or a lock marker (`!`/`*`) / empty field.
    pub hash: String,
    /// Days since the epoch of the last change.
    pub last_change: i64,
    /// Minimum days between changes.
    pub min: i64,
    /// Maximum days a password remains valid.
    pub max: i64,
    /// Warning days before expiry.
    pub warn: i64,
    /// Days after expiry before the account is disabled.
    pub inactive: i64,
    /// Absolute expiry day.
    pub expire: i64,
    /// Reserved field.
    pub reserved: i64,
}

impl ShadowEntry {
    /// A new locked record with the 0.2.0 default aging fields.
    #[must_use]
    pub fn locked(name: &str) -> Self {
        Self {
            name: name.to_string(),
            hash: "!".to_string(),
            last_change: 0,
            min: 0,
            max: 99999,
            warn: 7,
            inactive: -1,
            expire: -1,
            reserved: 0,
        }
    }

    /// Parse one shadow line. `line_no` is one-based and only used in diagnostics.
    ///
    /// # Errors
    /// [`DbError::FieldCount`] for a field-count mismatch and [`DbError::InvalidField`] for a
    /// malformed name, hash, or numeric field.
    pub fn parse(line: &str, line_no: usize) -> Result<Self, DbError> {
        let fields = fields(line, line_no, DbFile::Shadow, FIELDS)?;
        let name = fields.first().copied().unwrap_or("");
        if !valid_name(name) {
            return Err(DbError::InvalidField {
                file: DbFile::Shadow,
                line: line_no,
                field: "name",
            });
        }
        let hash = fields.get(1).copied().unwrap_or("");
        if !valid_hash(hash) {
            return Err(DbError::InvalidField {
                file: DbFile::Shadow,
                line: line_no,
                field: "hash",
            });
        }
        let number = |index: usize, field: &'static str| {
            integer(
                fields.get(index).copied().unwrap_or(""),
                DbFile::Shadow,
                line_no,
                field,
            )
        };
        let optional = |index: usize, field: &'static str| -> Result<i64, DbError> {
            let text = fields.get(index).copied().unwrap_or("");
            if text.is_empty() {
                return Ok(-1);
            }
            let value = text.parse::<i64>().map_err(|_| DbError::InvalidField {
                file: DbFile::Shadow,
                line: line_no,
                field,
            })?;
            if value < -1 {
                return Err(DbError::InvalidField {
                    file: DbFile::Shadow,
                    line: line_no,
                    field,
                });
            }
            Ok(value)
        };
        Ok(Self {
            name: name.to_string(),
            hash: hash.to_string(),
            last_change: number(2, "last_change")?,
            min: number(3, "min")?,
            max: number(4, "max")?,
            warn: number(5, "warn")?,
            inactive: optional(6, "inactive")?,
            expire: optional(7, "expire")?,
            reserved: number(8, "reserved")?,
        })
    }

    /// Render the record in shadow format.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.name,
            self.hash,
            self.last_change,
            self.min,
            self.max,
            self.warn,
            self.inactive,
            self.expire,
            self.reserved
        )
    }

    /// True when the record can never verify a password.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        self.hash.is_empty() || self.hash.starts_with('!') || self.hash.starts_with('*')
    }

    /// Prefix the hash with the traditional lock marker.
    pub fn lock(&mut self) {
        if self.hash.is_empty() {
            self.hash = "!".to_string();
        } else if !self.hash.starts_with('!') {
            self.hash.insert(0, '!');
        }
    }

    /// Remove one lock marker.
    ///
    /// # Errors
    /// [`DbError::InvalidHash`] when the record has no removable `$6$` hash behind the marker.
    pub fn unlock(&mut self) -> Result<(), DbError> {
        let Some(rest) = self.hash.strip_prefix('!') else {
            return Err(DbError::InvalidHash);
        };
        if rest.is_empty() || rest.starts_with('*') {
            return Err(DbError::InvalidHash);
        }
        self.hash = rest.to_string();
        Ok(())
    }
}

#[cfg(test)]
#[path = "shadow_ut.rs"]
mod tests;
