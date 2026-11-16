// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/etc/group` records.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::error::{DbError, DbFile};
use crate::validate::{decimal, fields, valid_name};

/// Number of colon-separated fields in a group record.
pub const FIELDS: usize = 4;

/// One `/etc/group` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupEntry {
    /// Group name.
    pub name: String,
    /// Password marker; `x` or `*` is accepted.
    pub passwd: String,
    /// Numeric group id.
    pub gid: u32,
    /// Explicit member names in file order.
    pub members: Vec<String>,
}

impl GroupEntry {
    /// Parse one group line. `line_no` is one-based and only used in diagnostics.
    ///
    /// # Errors
    /// [`DbError::FieldCount`] for a field-count mismatch and [`DbError::InvalidField`] for a
    /// malformed name, password marker, id or member name.
    pub fn parse(line: &str, line_no: usize) -> Result<Self, DbError> {
        let fields = fields(line, line_no, DbFile::Group, FIELDS)?;
        let name = fields.first().copied().unwrap_or("");
        if !valid_name(name) {
            return Err(DbError::InvalidField {
                file: DbFile::Group,
                line: line_no,
                field: "name",
            });
        }
        let passwd = fields.get(1).copied().unwrap_or("");
        if !matches!(passwd, "x" | "*") {
            return Err(DbError::InvalidField {
                file: DbFile::Group,
                line: line_no,
                field: "passwd",
            });
        }
        let gid = decimal(
            fields.get(2).copied().unwrap_or(""),
            DbFile::Group,
            line_no,
            "gid",
        )?;
        let members_field = fields.get(3).copied().unwrap_or("");
        let mut members = Vec::new();
        for member in members_field.split(',').filter(|name| !name.is_empty()) {
            if !valid_name(member) || members.iter().any(|seen: &String| seen == member) {
                return Err(DbError::InvalidField {
                    file: DbFile::Group,
                    line: line_no,
                    field: "member",
                });
            }
            members.push(member.to_string());
        }
        Ok(Self {
            name: name.to_string(),
            passwd: passwd.to_string(),
            gid,
            members,
        })
    }

    /// Render the record in group format.
    #[must_use]
    pub fn render(&self) -> String {
        let members = self.members.join(",");
        format!("{}:{}:{}:{}", self.name, self.passwd, self.gid, members)
    }

    /// True when `name` is an explicit member of this group.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.members.iter().any(|member| member == name)
    }

    /// Append `name` once; returns true when the record changed.
    pub fn add_member(&mut self, name: &str) -> bool {
        if self.contains(name) {
            return false;
        }
        self.members.push(name.to_string());
        true
    }

    /// Remove `name`; returns true when the record changed.
    pub fn remove_member(&mut self, name: &str) -> bool {
        let before = self.members.len();
        self.members.retain(|member| member != name);
        self.members.len() != before
    }
}

#[cfg(test)]
#[path = "group_ut.rs"]
mod tests;
