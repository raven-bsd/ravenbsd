// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `/etc/shadow` record format and the root-only verification helper.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::errors::AuthError;
use crate::hash::{DEFAULT_ROOT_HASH, verify_password};

/// One `/etc/shadow` record.
///
/// The nine fields are `name:hash:last_change:min:max:warn:inactive:expire:reserved`. Empty
/// `last_change`/`min`/`max`/`warn` fields read as zero; an empty `inactive`/`expire` field is
/// `-1`, the shadow convention for "not set" (zero means day 0 to OpenSSH).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowEntry {
    /// Account name.
    pub name: String,
    /// `$6$...` password hash, or a locked marker (`!`/`*`).
    pub hash: String,
    /// Days since the epoch of the last password change.
    pub last_change: i64,
    /// Minimum days between changes.
    pub min: i64,
    /// Maximum days a password remains valid.
    pub max: i64,
    /// Days of warning before expiry.
    pub warn: i64,
    /// Days after expiry before the account is disabled.
    pub inactive: i64,
    /// Absolute expiry day.
    pub expire: i64,
    /// Reserved field.
    pub reserved: i64,
}

impl ShadowEntry {
    /// Parse one shadow line.
    ///
    /// # Errors
    /// [`AuthError::InvalidRecord`] for a field-count mismatch and [`AuthError::InvalidNumber`]
    /// for a non-integer numeric field.
    pub fn parse(line: &str) -> Result<Self, AuthError> {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() != 9 {
            return Err(AuthError::InvalidRecord);
        }
        let name = fields.first().copied().unwrap_or("").to_string();
        if name.is_empty() {
            return Err(AuthError::InvalidRecord);
        }
        let hash = fields.get(1).copied().unwrap_or("").to_string();
        let number = |index: usize| -> Result<i64, AuthError> {
            let field = fields.get(index).copied().unwrap_or("");
            if field.is_empty() {
                Ok(0)
            } else {
                field.parse::<i64>().map_err(|_| AuthError::InvalidNumber)
            }
        };
        let optional = |index: usize| -> Result<i64, AuthError> {
            let field = fields.get(index).copied().unwrap_or("");
            if field.is_empty() {
                Ok(-1)
            } else {
                let value = field.parse::<i64>().map_err(|_| AuthError::InvalidNumber)?;
                if value < -1 {
                    return Err(AuthError::InvalidNumber);
                }
                Ok(value)
            }
        };
        Ok(Self {
            name,
            hash,
            last_change: number(2)?,
            min: number(3)?,
            max: number(4)?,
            warn: number(5)?,
            inactive: optional(6)?,
            expire: optional(7)?,
            reserved: number(8)?,
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

    /// Verify `password` for the named account.
    #[must_use]
    pub fn verify(&self, name: &str, password: &str) -> bool {
        if self.name != name {
            return false;
        }
        if self.hash.is_empty() || self.hash.starts_with('!') || self.hash.starts_with('*') {
            return false;
        }
        verify_password(password, &self.hash)
    }
}

/// A shadow line for `root` with the supplied hash and the standard release fields.
#[must_use]
pub fn root_line(hash: &str) -> String {
    format!("root:{hash}:20000:0:99999:7:-1:-1:-1")
}

/// The default `root` shadow line generated from the release password at build time.
#[must_use]
pub fn default_root_shadow() -> String {
    root_line(DEFAULT_ROOT_HASH)
}

/// Verify a root password against one shadow line; any other account is rejected.
#[must_use]
pub fn verify_root_shadow(line: &str, password: &str) -> bool {
    match ShadowEntry::parse(line) {
        Ok(entry) => entry.verify("root", password),
        Err(_) => false,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "shadow_ut.rs"]
mod tests;
