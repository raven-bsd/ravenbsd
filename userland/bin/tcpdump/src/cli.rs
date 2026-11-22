// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line parsing shared by the host and Raven targets.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::Timestamps;

/// Parsed `tcpdump` options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `-r <file>`.
    pub read: Option<String>,
    /// `-w <file>`.
    pub write: Option<String>,
    /// `-i <interface>`.
    pub interface: Option<String>,
    /// `-c <count>`.
    pub count: Option<usize>,
    /// `-n`/`-nn`: numeric addresses and ports.
    pub numeric: bool,
    /// `-v` repeat count.
    pub verbose: u8,
    /// `-t`/`-tt`.
    pub timestamps: Timestamps,
    /// `-S`: absolute TCP sequence numbers.
    pub absolute_seq: bool,
    /// The trailing filter expression.
    pub filter: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            read: None,
            write: None,
            interface: None,
            count: None,
            numeric: false,
            verbose: 0,
            timestamps: Timestamps::None,
            absolute_seq: false,
            filter: None,
        }
    }
}

/// A command-line parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// An option needs an argument.
    MissingValue(String),
    /// An option is not implemented.
    UnknownOption(String),
    /// `-c` is not a positive integer.
    InvalidCount(String),
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingValue(option) => write!(f, "option {option} needs a value"),
            Self::UnknownOption(option) => write!(f, "unknown option {option}"),
            Self::InvalidCount(value) => write!(f, "invalid packet count {value}"),
        }
    }
}

/// Parse the arguments after the program name.
///
/// # Errors
/// Returns [`ParseError`] for a missing option value, an unknown option or an
/// invalid `-c` count.
pub fn parse<I>(args: I) -> Result<Options, ParseError>
where
    I: IntoIterator<Item = String>,
{
    let values: Vec<String> = args.into_iter().collect();
    let mut options = Options::default();
    let mut filter_tokens = Vec::new();
    let mut index = 0usize;
    let mut end_options = false;
    let mut t_flags = 0u8;
    while index < values.len() {
        let value = &values[index];
        if end_options || value == "-" || !value.starts_with('-') {
            filter_tokens.push(value.clone());
            index += 1;
            continue;
        }
        if value == "--" {
            end_options = true;
            index += 1;
            continue;
        }
        if value.starts_with("--") {
            return Err(ParseError::UnknownOption(value.clone()));
        }
        let mut characters = value[1..].chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                'n' => options.numeric = true,
                'v' => options.verbose = options.verbose.saturating_add(1),
                'S' => options.absolute_seq = true,
                't' => {
                    t_flags = t_flags.saturating_add(1);
                    options.timestamps = if t_flags >= 2 {
                        Timestamps::Unix
                    } else {
                        Timestamps::None
                    };
                }
                'r' | 'w' | 'i' | 'c' => {
                    let rest: String = characters.collect();
                    let argument = if rest.is_empty() {
                        index += 1;
                        values
                            .get(index)
                            .cloned()
                            .ok_or_else(|| ParseError::MissingValue(format!("-{character}")))?
                    } else {
                        rest
                    };
                    match character {
                        'r' => options.read = Some(argument),
                        'w' => options.write = Some(argument),
                        'i' => options.interface = Some(argument),
                        'c' => options.count = Some(parse_count(&argument)?),
                        _ => unreachable!(),
                    }
                    break;
                }
                other => {
                    return Err(ParseError::UnknownOption(format!("-{other}")));
                }
            }
        }
        index += 1;
    }
    options.filter = filter_expression(&filter_tokens);
    Ok(options)
}

fn parse_count(value: &str) -> Result<usize, ParseError> {
    let count = value
        .parse::<usize>()
        .map_err(|_| ParseError::InvalidCount(value.to_string()))?;
    if count == 0 {
        Err(ParseError::InvalidCount(value.to_string()))
    } else {
        Ok(count)
    }
}

/// Join the trailing filter tokens.
pub(crate) fn filter_expression(tokens: &[String]) -> Option<String> {
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

#[cfg(test)]
#[path = "cli_ut.rs"]
mod tests;
