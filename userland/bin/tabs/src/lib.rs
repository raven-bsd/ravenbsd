// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `tabs` escape generation.
//!
//! Supported: the default "set tab stops" form and `-c` cursor-motion form for an explicit stop
//! list. `-T` accepts a terminal type but the implementation is ANSI-only; the other historical
//! styles return a typed usage error instead of a silent success.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Usage line.
pub const USAGE: &str = "tabs [-c] [-T type] [stop ...]";

/// Terminal style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Set tab stops (`ESC [ n g`).
    Stops,
    /// Move to each stop (`ESC [ n G`).
    Cursor,
}

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// A stop is not a column number.
    BadStop,
    /// A stop is outside 1..=999.
    StopRange,
    /// A non-supported historical style was requested.
    UnsupportedStyle,
    /// `-T` without a value.
    MissingTerminal,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Output style.
    pub style: Style,
    /// Tab stops in columns (1-based).
    pub stops: Vec<u16>,
}

/// Parse `tabs` arguments (without `argv[0]`).
pub fn parse_args<'a, I>(args: I) -> Result<Options, ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut style = Style::Stops;
    let mut stops = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg {
            "-c" => style = Style::Cursor,
            "-T" => {
                iter.next().ok_or(ParseError::MissingTerminal)?;
            }
            "-a" | "-f" | "-p" | "-s" | "-u" => return Err(ParseError::UnsupportedStyle),
            other if other.starts_with('+') || other.starts_with('-') && other != "-" => {
                return Err(ParseError::UnsupportedStyle);
            }
            other => {
                let stop = other.parse::<u16>().map_err(|_| ParseError::BadStop)?;
                if stop == 0 || stop > 999 {
                    return Err(ParseError::StopRange);
                }
                stops.push(stop);
            }
        }
    }
    if stops.is_empty() {
        stops.extend((1..=10).map(|step| step * 8));
    }
    Ok(Options { style, stops })
}

/// Render the escape sequence for `options`.
pub fn render(options: &Options) -> String {
    let mut out = String::new();
    match options.style {
        Style::Stops => {
            out.push_str("\x1b[3g");
            for stop in &options.stops {
                out.push_str(&format!("\x1b[{stop}g"));
            }
        }
        Style::Cursor => {
            for stop in &options.stops {
                out.push_str(&format!("\x1b[{stop}G"));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
