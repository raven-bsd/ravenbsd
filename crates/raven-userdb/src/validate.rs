// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared field validation rules.
//!
//! The rules are deliberately conservative: they reject bytes that cannot be represented in a
//! colon-separated line and paths that could escape the intended directory. They do not try to
//! guess a site's naming policy beyond the 0.2.0 model.

use alloc::vec::Vec;

use crate::error::DbError;

/// Maximum account/group name length in bytes.
pub const MAX_NAME: usize = 31;
/// Maximum gecos length in bytes.
pub const MAX_GECOS: usize = 127;

/// True for a valid account or group name.
///
/// Names start with an ASCII letter or underscore and contain only ASCII letters, digits,
/// underscore, dot and hyphen. This mirrors the fixture contract and keeps option parsing
/// unambiguous.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    if name.len() > MAX_NAME {
        return false;
    }
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() || first == b'_' => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// True when a gecos field is representable and within the 0.2.0 length limit.
#[must_use]
pub fn valid_gecos(gecos: &str) -> bool {
    gecos.len() <= MAX_GECOS && clean_field(gecos)
}

/// True for an absolute path with no parent-component escape.
///
/// An empty string is accepted because `nobody` uses an empty home and shell.
#[must_use]
pub fn valid_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if !path.starts_with('/') || !clean_field(path) {
        return false;
    }
    !path
        .split('/')
        .any(|component| component == ".." || component == ".")
}

/// True when a shadow hash is a SHA-512-crypt record or a lock marker.
#[must_use]
pub fn valid_hash(hash: &str) -> bool {
    if hash.is_empty() || hash.starts_with('*') {
        return true;
    }
    if let Some(rest) = hash.strip_prefix('!') {
        return rest.is_empty() || rest.starts_with("$6$");
    }
    hash.starts_with("$6$") && clean_field(hash)
}

/// True when text contains no colon, NUL or line break.
fn clean_field(text: &str) -> bool {
    !text
        .bytes()
        .any(|byte| matches!(byte, b':' | b'\0' | b'\n' | b'\r'))
}

/// Split one account file into non-empty logical records.
pub(crate) fn records(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(index, line)| (index + 1, line))
}

/// Split a colon-separated record and check its field count.
pub(crate) fn fields(
    line: &str,
    line_no: usize,
    file: crate::error::DbFile,
    expected: usize,
) -> Result<Vec<&str>, DbError> {
    let fields: Vec<&str> = line.split(':').collect();
    if fields.len() != expected {
        return Err(DbError::FieldCount {
            file,
            line: line_no,
            expected,
            found: fields.len(),
        });
    }
    Ok(fields)
}

/// Parse a non-empty decimal id.
pub(crate) fn decimal(
    text: &str,
    file: crate::error::DbFile,
    line: usize,
    field: &'static str,
) -> Result<u32, DbError> {
    text.parse::<u32>()
        .map_err(|_| DbError::InvalidField { file, line, field })
}

/// Parse a shadow integer; an empty field is zero.
pub(crate) fn integer(
    text: &str,
    file: crate::error::DbFile,
    line: usize,
    field: &'static str,
) -> Result<i64, DbError> {
    if text.is_empty() {
        return Ok(0);
    }
    let value = text
        .parse::<i64>()
        .map_err(|_| DbError::InvalidField { file, line, field })?;
    if value < 0 {
        return Err(DbError::InvalidField { file, line, field });
    }
    Ok(value)
}

#[cfg(test)]
#[path = "validate_ut.rs"]
mod tests;
