// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SCCS `unget` argument handling and p-file mapping.
//!
//! The SCCS history model is outside this utility: `unget` maps an SCCS file (`s.foo`) to its
//! pending p-file (`p.foo`) and restores it. The bounded implementation accepts the POSIX
//! `-r`, `-s` and `-n` options, reports typed usage errors and never fabricates a revision.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Usage line.
pub const USAGE: &str = "unget [-ns] [-r SID] file ...";

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// No file operand.
    MissingFile,
    /// Unknown option.
    UnknownOption,
    /// `-r` without a SID.
    MissingSid,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Suppress the normal output.
    pub suppress: bool,
    /// Keep the p-file after the restore.
    pub keep: bool,
    /// Optional SCCS revision id.
    pub sid: Option<&'a str>,
    /// SCCS file operands.
    pub files: Vec<&'a str>,
}

/// Parse `unget` arguments (without `argv[0]`).
pub fn parse_args<'a, I>(args: I) -> Result<Options<'a>, ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut suppress = false;
    let mut keep = false;
    let mut sid = None;
    let mut files = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg {
            "-s" => suppress = true,
            "-n" => keep = true,
            "-r" => sid = Some(iter.next().ok_or(ParseError::MissingSid)?),
            other if other.starts_with('-') && other != "-" => {
                return Err(ParseError::UnknownOption);
            }
            other => files.push(other),
        }
    }
    if files.is_empty() {
        return Err(ParseError::MissingFile);
    }
    Ok(Options {
        suppress,
        keep,
        sid,
        files,
    })
}

/// Map an SCCS file to its pending p-file.
///
/// `s.foo` maps to `p.foo`; a name that does not start with `s.` maps to `p.<name>`.
pub fn pending_path(file: &str) -> String {
    let (dir, base) = match file.rfind('/') {
        Some(index) => (&file[..=index], &file[index + 1..]),
        None => ("", file),
    };
    let rest = base.strip_prefix("s.").unwrap_or(base);
    let mut out = String::with_capacity(dir.len() + rest.len() + 2);
    out.push_str(dir);
    out.push('p');
    if !rest.is_empty() {
        out.push('.');
        out.push_str(rest);
    }
    out
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
