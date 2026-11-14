// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for the ATF test-program listing protocol (`program -l`).
//!
//! The format is the one printed by `atf-c/detail/tp_main.c:255-290`: a Content-Type header, an
//! empty line, then one block per test case where the first key is `ident` and blocks are
//! separated by one empty line.

use std::collections::BTreeMap;
use std::fmt;

use crate::TpCase;

/// The only accepted Content-Type line.
pub const CONTENT_TYPE: &str = "Content-Type: application/X-atf-tp; version=\"1\"";

/// A listing parser error with the 1-based input line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtoError {
    /// 1-based line number in the input.
    pub line: usize,
    /// What was wrong.
    pub message: String,
}

impl ProtoError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ProtoError {}

/// Parse `program -l` output.
pub fn parse_list(text: &str) -> Result<Vec<TpCase>, ProtoError> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let Some(header) = lines.first() else {
        return Err(ProtoError::new(1, "empty listing"));
    };
    if *header != CONTENT_TYPE {
        return Err(ProtoError::new(
            1,
            format!("expected `{CONTENT_TYPE}`, got `{}`", truncate(header)),
        ));
    }
    match lines.get(1) {
        Some(&"") => {}
        Some(other) => {
            return Err(ProtoError::new(
                2,
                format!(
                    "expected the empty line after Content-Type, got `{}`",
                    truncate(other)
                ),
            ));
        }
        None => {
            return Err(ProtoError::new(
                2,
                "missing the empty line after Content-Type",
            ));
        }
    }

    let mut blocks: Vec<(usize, Vec<&str>)> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut current_start = 3;
    for (index, line) in lines.iter().enumerate().skip(2) {
        let number = index + 1;
        if line.is_empty() {
            if current.is_empty() {
                return Err(ProtoError::new(
                    number,
                    "two consecutive empty lines between blocks",
                ));
            }
            blocks.push((current_start, std::mem::take(&mut current)));
            current_start = number + 1;
        } else {
            if current.is_empty() {
                current_start = number;
            }
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push((current_start, current));
    }
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    let mut cases = Vec::new();
    let mut seen = BTreeMap::new();
    for (start, block) in blocks {
        let mut meta = BTreeMap::new();
        for (offset, line) in block.iter().enumerate() {
            let number = start + offset;
            let Some((key, value)) = line.split_once(": ") else {
                return Err(ProtoError::new(
                    number,
                    format!("metadata line `{}` is not `key: value`", truncate(line)),
                ));
            };
            if key.is_empty() || key.trim() != key {
                return Err(ProtoError::new(number, "empty or padded metadata key"));
            }
            if meta.insert(key.to_string(), value.to_string()).is_some() {
                return Err(ProtoError::new(
                    number,
                    format!("duplicate metadata key `{key}`"),
                ));
            }
        }
        let Some(ident) = meta.get("ident").cloned() else {
            return Err(ProtoError::new(start, "block does not start with `ident`"));
        };
        if block
            .first()
            .is_none_or(|line| !line.starts_with("ident: "))
        {
            return Err(ProtoError::new(
                start,
                "the first key of a block must be `ident`",
            ));
        }
        if let Some(previous) = seen.insert(ident.clone(), start) {
            return Err(ProtoError::new(
                start,
                format!("duplicate ident `{ident}` (first at line {previous})"),
            ));
        }
        cases.push(TpCase { ident, meta });
    }
    Ok(cases)
}

/// Truncate a line for an error message.
fn truncate(text: &str) -> String {
    text.chars().take(100).collect()
}

#[cfg(test)]
#[path = "atf_proto_ut.rs"]
mod tests;
