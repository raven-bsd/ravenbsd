// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure pattern selection and argument handling for `grep`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_libc_regex::consts::{REG_EXTENDED, REG_ICASE, REG_NOMATCH, REG_NOSPEC};
use raven_libc_regex::locale::ByteLocale;
use raven_libc_regex::{RegError, Regex, regcomp, regexec};

/// Parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Pattern.
    pub pattern: &'a str,
    /// Input files.
    pub files: Vec<&'a str>,
    /// `-i`.
    pub ignore_case: bool,
    /// `-v`.
    pub invert: bool,
    /// `-n`.
    pub line_number: bool,
    /// `-l`.
    pub files_with_matches: bool,
    /// `-q`.
    pub quiet: bool,
    /// `-c`.
    pub count: bool,
    /// `-E`.
    pub extended: bool,
    /// `-F`.
    pub fixed: bool,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown short option.
    UnknownOption(char),
    /// No pattern was supplied.
    MissingPattern,
}

/// Parse `grep [-i -v -n -l -q -c -E -F] pattern [file ...]`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options::default();
    let mut parse_options = true;
    let mut pattern = None;
    for argument in args {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                match option {
                    'i' => options.ignore_case = true,
                    'v' => options.invert = true,
                    'n' => options.line_number = true,
                    'l' => options.files_with_matches = true,
                    'q' => options.quiet = true,
                    'c' => options.count = true,
                    'E' => options.extended = true,
                    'F' => options.fixed = true,
                    other => return Err(UsageError::UnknownOption(other)),
                }
            }
            continue;
        }
        if pattern.is_none() {
            pattern = Some(argument);
        } else {
            options.files.push(argument);
        }
    }
    let pattern = pattern.ok_or(UsageError::MissingPattern)?;
    options.pattern = pattern;
    Ok(options)
}

/// Compile the pattern with the requested POSIX flags.
pub fn compile(options: &Options<'_>) -> Result<Regex, RegError> {
    let mut flags = 0;
    if options.extended {
        flags |= REG_EXTENDED;
    }
    if options.ignore_case {
        flags |= REG_ICASE;
    }
    if options.fixed {
        flags |= REG_NOSPEC;
    }
    regcomp(options.pattern.as_bytes(), flags, &ByteLocale)
}

/// What one input produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// At least one line matched (after inversion).
    pub matched: bool,
    /// Number of selected lines.
    pub count: usize,
    /// Rendered output lines.
    pub output: Vec<String>,
}

/// Process one input buffer. `show_name` prefixes normal/count output with `name`.
pub fn process(
    regex: &Regex,
    name: Option<&str>,
    show_name: bool,
    data: &[u8],
    options: &Options<'_>,
) -> Result<Report, RegError> {
    let mut lines: Vec<&[u8]> = if data.is_empty() {
        Vec::new()
    } else {
        data.split(|byte| *byte == b'\n').collect()
    };
    if data.ends_with(b"\n") {
        lines.pop();
    }
    let prefix = match (show_name, name) {
        (true, Some(name)) => format!("{name}:"),
        _ => String::new(),
    };
    let mut report = Report {
        matched: false,
        count: 0,
        output: Vec::new(),
    };
    for (index, line) in lines.iter().enumerate() {
        let found = match regexec(regex, line, &mut [], 0, &ByteLocale) {
            Ok(()) => true,
            Err(error) if error.0 == REG_NOMATCH => false,
            Err(error) => return Err(error),
        };
        let selected = if options.invert { !found } else { found };
        if !selected {
            continue;
        }
        report.matched = true;
        report.count = report.count.saturating_add(1);
        if options.quiet || options.files_with_matches {
            break;
        }
        if options.count {
            continue;
        }
        let mut text = String::new();
        if show_name {
            text.push_str(&prefix);
        }
        if options.line_number {
            text.push_str(&format!("{}:", index.saturating_add(1)));
        }
        text.push_str(&String::from_utf8_lossy(line));
        report.output.push(text);
    }
    if options.count {
        let mut text = String::new();
        if show_name {
            text.push_str(&prefix);
        }
        text.push_str(&report.count.to_string());
        report.output.push(text);
    } else if options.files_with_matches
        && report.matched
        && let Some(name) = name
    {
        report.output.push(name.to_string());
    }
    Ok(report)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
