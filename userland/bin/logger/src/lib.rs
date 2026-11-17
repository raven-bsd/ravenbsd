// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure priority/tag parsing for `logger`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The synopsis printed after `usage: logger`.
pub const USAGE: &str = "[-p priority] [-t tag] [-f file] [message ...]";

/// A parsed `logger` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Syslog priority value (`facility * 8 + severity`).
    pub priority: u8,
    /// Log name/tag.
    pub tag: Option<String>,
    /// File whose contents are logged instead of the message operands.
    pub file: Option<String>,
    /// Message operands.
    pub message: Vec<String>,
}

/// Why a `logger` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// Unknown option.
    UnknownOption,
    /// `-p`, `-t` or `-f` without a value.
    MissingValue,
    /// No message operands and no file.
    MissingMessage,
    /// A bad priority value.
    BadPriority,
}

const FACILITIES: &[(&str, u8)] = &[
    ("kern", 0),
    ("user", 1),
    ("mail", 2),
    ("daemon", 3),
    ("auth", 4),
    ("syslog", 5),
    ("lpr", 6),
    ("news", 7),
    ("uucp", 8),
    ("cron", 9),
    ("authpriv", 10),
    ("ftp", 11),
    ("local0", 16),
    ("local1", 17),
    ("local2", 18),
    ("local3", 19),
    ("local4", 20),
    ("local5", 21),
    ("local6", 22),
    ("local7", 23),
];

const SEVERITIES: &[(&str, u8)] = &[
    ("emerg", 0),
    ("alert", 1),
    ("crit", 2),
    ("err", 3),
    ("warning", 4),
    ("notice", 5),
    ("info", 6),
    ("debug", 7),
];

fn facility(name: &str) -> Option<u8> {
    FACILITIES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, value)| *value)
}

fn severity(name: &str) -> Option<u8> {
    SEVERITIES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, value)| *value)
}

/// Parse a syslog priority: a number, `facility.severity`, a severity, or a facility.
pub fn parse_priority(text: &str) -> Option<u8> {
    if let Ok(number) = text.parse::<u16>() {
        return u8::try_from(number).ok().filter(|value| *value <= 191);
    }
    if let Some((facility_name, severity_name)) = text.split_once('.') {
        let facility = facility(facility_name)?;
        let severity = severity(severity_name)?;
        return facility.checked_mul(8)?.checked_add(severity);
    }
    if let Some(severity) = severity(text) {
        return 8u8.checked_add(severity);
    }
    facility(text).map(|facility| facility.saturating_mul(8).saturating_add(5))
}

/// Parse `logger [-p priority] [-t tag] [-f file] [message ...]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut priority = 13u8;
    let mut tag = None;
    let mut file = None;
    let mut message = Vec::new();
    let mut args = args.into_iter().peekable();
    let mut parsing = true;
    while let Some(argument) = args.next() {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && (argument == "-p" || argument == "--priority") {
            let value = args.next().ok_or(UsageError::MissingValue)?;
            priority = parse_priority(value).ok_or(UsageError::BadPriority)?;
            continue;
        }
        if parsing && (argument == "-t" || argument == "--tag") {
            tag = Some(args.next().ok_or(UsageError::MissingValue)?.to_string());
            continue;
        }
        if parsing && (argument == "-f" || argument == "--file") {
            file = Some(args.next().ok_or(UsageError::MissingValue)?.to_string());
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" {
            return Err(UsageError::UnknownOption);
        }
        message.push(argument.to_string());
    }
    if message.is_empty() && file.is_none() {
        return Err(UsageError::MissingMessage);
    }
    Ok(Options {
        priority,
        tag,
        file,
        message,
    })
}

/// Format the log payload as `<priority>message`.
#[must_use]
pub fn payload(priority: u8, message: &str) -> String {
    let mut text = alloc::format!("<{priority}>");
    text.push_str(message);
    text
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
