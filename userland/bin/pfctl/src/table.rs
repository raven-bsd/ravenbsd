// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Table file parser.
//!
//! The format is the `pfctl -t <name> -T add -f <file>` input from the FreeBSD
//! donor `sbin/pfctl/pfctl_table.c` at `133344405f` (BSD-2-Clause): one address
//! per line, an optional `!` negation, `#` comments. The parser is bounded:
//! a malformed line is an error with its line number, never a panic.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::is_address_literal;
use crate::parser::ParseError;

/// One table file entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableFileEntry {
    /// True for a `!`-negated entry.
    pub negative: bool,
    /// The validated address literal.
    pub address: String,
}

/// A parsed table file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableFile {
    /// Entries in file order.
    pub entries: Vec<TableFileEntry>,
}

/// Parse a table file.
pub fn parse_table_file(text: &str) -> Result<TableFile, ParseError> {
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let line = match line.split_once('#') {
            Some((before, _)) => before,
            None => line,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (negative, rest) = match trimmed.strip_prefix('!') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, trimmed),
        };
        let mut fields = rest.split_whitespace();
        let Some(address) = fields.next() else {
            return Err(ParseError {
                file: None,
                line: number,
                message: String::from("`!' without an address"),
            });
        };
        if let Some(extra) = fields.next() {
            return Err(ParseError {
                file: None,
                line: number,
                message: alloc::format!("unexpected `{extra}' after `{address}'"),
            });
        }
        if !is_address_literal(address) {
            return Err(ParseError {
                file: None,
                line: number,
                message: alloc::format!("invalid address `{address}'"),
            });
        }
        entries.push(TableFileEntry {
            negative,
            address: address.to_string(),
        });
    }
    Ok(TableFile { entries })
}

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;
