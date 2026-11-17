// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `strip`: neutralize symbol and debug section headers.
//!
//! The tool never moves file contents or program headers. It clears the section headers of
//! `.symtab`, its dedicated `.strtab` and debug sections, leaving the section-header table itself
//! in place so every other offset stays valid. A stripped executable therefore still loads.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

use raven_elf::inspect::{Object, SHT_STRTAB, SHT_SYMTAB};

pub use raven_elf::inspect::Class;

/// What to remove.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Remove symbols, their string table and debug sections (the default and `-s`).
    #[default]
    All,
    /// Remove only debug sections (`-g`).
    Debug,
}

/// Parsed `strip` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Strip mode.
    pub mode: Mode,
    /// `-o outfile`: write here and leave the input untouched.
    pub output: Option<&'a str>,
    /// Input files.
    pub operands: Vec<&'a str>,
}

/// Why a command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option letter is not supported.
    UnknownOption(char),
    /// No input file (or no `-o` argument) was supplied.
    MissingOperand,
    /// `-o` cannot be combined with multiple input files.
    OutputWithMultipleFiles,
}

/// Why a file could not be stripped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripError {
    /// The input is not a supported ELF32/ELF64 object.
    Malformed,
    /// The rewritten file did not reparse with the expected section headers.
    VerifyFailed,
}

/// What one strip operation removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    /// ELF class of the input.
    pub class: Class,
    /// Number of section headers neutralized.
    pub blanked: usize,
}

/// Parse `strip [-g|-s] [-o outfile] [--] file...`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let args: Vec<&'a str> = args.into_iter().collect();
    let mut options = Options::default();
    let mut parse_options = true;
    let mut index = 0usize;
    while index < args.len() {
        let argument = args[index];
        index += 1;
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            let body = &argument[1..];
            for (offset, option) in body.char_indices() {
                match option {
                    'g' => options.mode = Mode::Debug,
                    's' => options.mode = Mode::All,
                    'o' => {
                        let rest = &body[offset + 1..];
                        if rest.is_empty() {
                            let value = args.get(index).ok_or(UsageError::MissingOperand)?;
                            index += 1;
                            options.output = Some(value);
                        } else {
                            options.output = Some(rest);
                        }
                        break;
                    }
                    other => return Err(UsageError::UnknownOption(other)),
                }
            }
            continue;
        }
        options.operands.push(argument);
    }
    if options.operands.is_empty() {
        return Err(UsageError::MissingOperand);
    }
    if options.output.is_some() && options.operands.len() > 1 {
        return Err(UsageError::OutputWithMultipleFiles);
    }
    Ok(options)
}

/// Strip `data` in place and return what was removed.
pub fn strip(data: &mut [u8], mode: Mode) -> Result<Report, StripError> {
    let object = Object::parse(data).map_err(|_| StripError::Malformed)?;
    let class = object.class();
    let section_count = object.section_count();
    let header_size = object.section_header_size();
    let mut candidates = Vec::new();

    for index in 0..section_count {
        let Some(section) = object.section(index) else {
            continue;
        };
        let name = section.name().unwrap_or("");
        if is_debug(name) {
            candidates.push(index);
        }
        if mode == Mode::All && section.section_type() == SHT_SYMTAB {
            candidates.push(index);
            let link = section.link() as usize;
            if let Some(table) = object.section(link)
                && table.section_type() == SHT_STRTAB
                && table.name() == Some(".strtab")
                && !linked_elsewhere(&object, link, index)
            {
                candidates.push(link);
            }
        }
    }
    candidates.sort_unstable();
    candidates.dedup();

    let mut ranges = Vec::new();
    for &index in &candidates {
        if index == 0 {
            continue;
        }
        let Some(section) = object.section(index) else {
            continue;
        };
        if section.name() == Some(".shstrtab") {
            continue;
        }
        ranges.push((section.header_offset(), header_size));
    }
    let blanked = ranges.len();
    for (offset, length) in ranges {
        let end = offset.checked_add(length).ok_or(StripError::Malformed)?;
        let header = data.get_mut(offset..end).ok_or(StripError::Malformed)?;
        header.fill(0);
    }

    let check = Object::parse(data).map_err(|_| StripError::VerifyFailed)?;
    if check.class() != class || check.section_count() != section_count {
        return Err(StripError::VerifyFailed);
    }
    for &index in &candidates {
        if index == 0 {
            continue;
        }
        if let Some(section) = check.section(index)
            && section.section_type() != 0
        {
            return Err(StripError::VerifyFailed);
        }
    }
    Ok(Report { class, blanked })
}

fn is_debug(name: &str) -> bool {
    name.starts_with(".debug")
        || name.starts_with(".zdebug")
        || name == ".stab"
        || name == ".stabstr"
        || name == ".line"
}

fn linked_elsewhere(object: &Object<'_>, target: usize, symtab: usize) -> bool {
    for index in 0..object.section_count() {
        if index == symtab || index == target {
            continue;
        }
        if let Some(section) = object.section(index)
            && section.link() as usize == target
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
