// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `more` paging model.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Default page height.
pub const DEFAULT_PAGE: usize = 24;

/// Parsed `more` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Lines per page.
    pub page: usize,
    /// Input files.
    pub files: Vec<String>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// `-n` has no valid value.
    BadPage,
    /// An unknown option was supplied.
    UnknownOption,
    /// No input file was supplied.
    MissingFile,
}

/// Parse `more [-n lines] file ...`.
///
/// # Errors
///
/// Returns [`UsageError`] for a bad page size, an unknown option or no file.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut page = DEFAULT_PAGE;
    let mut files = Vec::new();
    let mut args = args.into_iter();
    let mut options = true;
    while let Some(argument) = args.next() {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument == "-n" {
            let value = args.next().ok_or(UsageError::BadPage)?;
            page = value.parse().map_err(|_| UsageError::BadPage)?;
            if page == 0 {
                return Err(UsageError::BadPage);
            }
            continue;
        }
        if options && argument.starts_with('-') && argument.len() > 1 {
            return Err(UsageError::UnknownOption);
        }
        files.push(argument);
    }
    if files.is_empty() {
        return Err(UsageError::MissingFile);
    }
    Ok(Options { page, files })
}

/// The output/input seam used by the pager.
pub trait Pager {
    /// True when the output is a terminal.
    fn is_tty(&self) -> bool;
    /// Write text.
    fn write(&mut self, text: &str);
    /// Read one paging command; `None` means end of input.
    fn read_command(&mut self) -> Option<char>;
}

/// Page `input`; returns false when the user quit.
pub fn page(input: &str, page: usize, pager: &mut dyn Pager) -> bool {
    if !pager.is_tty() {
        pager.write(input);
        return true;
    }
    let lines: Vec<&str> = input.split_inclusive('\n').collect();
    let mut index = 0usize;
    let mut remaining = page.max(1);
    while index < lines.len() {
        if remaining == 0 {
            pager.write("--More--");
            match pager.read_command() {
                Some('q') | Some('Q') | None => return false,
                Some(' ') => remaining = page.max(1),
                Some('\n') | Some('\r') => remaining = 1,
                _ => remaining = page.max(1),
            }
            pager.write("\r        \r");
        }
        let take = remaining.min(lines.len() - index);
        for line in &lines[index..index + take] {
            pager.write(line);
        }
        index += take;
        remaining -= take;
    }
    true
}

/// Join file contents for paging.
#[must_use]
pub fn join(files: &[String]) -> String {
    let mut text = String::new();
    for file in files {
        text.push_str(file);
    }
    text
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
