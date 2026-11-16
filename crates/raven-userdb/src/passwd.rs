// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/etc/passwd` records.

use alloc::format;
use alloc::string::{String, ToString};

use crate::error::{DbError, DbFile};
use crate::validate::{decimal, fields, valid_gecos, valid_name, valid_path};

/// Number of colon-separated fields in a passwd record.
pub const FIELDS: usize = 7;

/// One `/etc/passwd` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswdEntry {
    /// Login name.
    pub name: String,
    /// Password marker; the 0.2.0 parser only accepts `x`.
    pub passwd: String,
    /// Numeric user id.
    pub uid: u32,
    /// Numeric primary group id.
    pub gid: u32,
    /// Free-form comment field.
    pub gecos: String,
    /// Home directory (empty for `nobody`).
    pub home: String,
    /// Login shell (empty for `nobody`).
    pub shell: String,
}

impl PasswdEntry {
    /// Parse one passwd line. `line_no` is one-based and only used in diagnostics.
    ///
    /// # Errors
    /// [`DbError::FieldCount`] for a field-count mismatch and [`DbError::InvalidField`] for a
    /// malformed name, password marker, id, gecos or path.
    pub fn parse(line: &str, line_no: usize) -> Result<Self, DbError> {
        let fields = fields(line, line_no, DbFile::Passwd, FIELDS)?;
        let name = fields.first().copied().unwrap_or("");
        if !valid_name(name) {
            return Err(DbError::InvalidField {
                file: DbFile::Passwd,
                line: line_no,
                field: "name",
            });
        }
        let passwd = fields.get(1).copied().unwrap_or("");
        if passwd != "x" {
            return Err(DbError::InvalidField {
                file: DbFile::Passwd,
                line: line_no,
                field: "passwd",
            });
        }
        let uid = decimal(
            fields.get(2).copied().unwrap_or(""),
            DbFile::Passwd,
            line_no,
            "uid",
        )?;
        let gid = decimal(
            fields.get(3).copied().unwrap_or(""),
            DbFile::Passwd,
            line_no,
            "gid",
        )?;
        let gecos = fields.get(4).copied().unwrap_or("");
        if !valid_gecos(gecos) {
            return Err(DbError::InvalidField {
                file: DbFile::Passwd,
                line: line_no,
                field: "gecos",
            });
        }
        let home = fields.get(5).copied().unwrap_or("");
        let shell = fields.get(6).copied().unwrap_or("");
        if !valid_path(home) || !valid_path(shell) {
            return Err(DbError::InvalidField {
                file: DbFile::Passwd,
                line: line_no,
                field: "path",
            });
        }
        Ok(Self {
            name: name.to_string(),
            passwd: passwd.to_string(),
            uid,
            gid,
            gecos: gecos.to_string(),
            home: home.to_string(),
            shell: shell.to_string(),
        })
    }

    /// Render the record in passwd format.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.name, self.passwd, self.uid, self.gid, self.gecos, self.home, self.shell
        )
    }
}

#[cfg(test)]
#[path = "passwd_ut.rs"]
mod tests;
