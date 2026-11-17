// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `nm`: list ELF32/ELF64 symbols.
//!
//! The library owns option parsing, symbol filtering, type letters and rendering. It reads the
//! object through [`raven_elf::inspect`] and never allocates more than one entry per symbol.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_elf::inspect::{
    Object, SHN_ABS, SHN_COMMON, SHN_UNDEF, SHT_NOBITS, STB_LOCAL, STB_WEAK, STT_FILE, STT_OBJECT,
    STT_SECTION, Symbol,
};

pub use raven_elf::inspect::Class;

const SHF_WRITE: u64 = 0x1;
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;

/// Parsed `nm` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// `-a`: include file and section symbols.
    pub all: bool,
    /// `-g`: only external (global and weak) symbols.
    pub external_only: bool,
    /// `-u`: only undefined symbols.
    pub undefined_only: bool,
    /// `-n`: sort numerically by symbol value.
    pub numeric: bool,
    /// `-p`: preserve symbol table order.
    pub no_sort: bool,
    /// `-r`: reverse the selected order.
    pub reverse: bool,
    /// Input files.
    pub operands: Vec<&'a str>,
}

/// Why a command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option letter is not supported.
    UnknownOption(char),
    /// No input file was supplied.
    MissingOperand,
}

/// Why an object could not be listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NmError {
    /// The input is not a supported ELF32/ELF64 object.
    Malformed,
}

/// One symbol ready for rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Symbol name.
    pub name: String,
    /// POSIX type letter.
    pub kind: char,
    /// Symbol value; zero when the symbol is undefined.
    pub value: u64,
    /// Symbol size.
    pub size: u64,
    /// Whether the value is undefined and must render as blanks.
    pub undefined: bool,
}

/// Parse `nm [-agunpr] [--] file...`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options::default();
    let mut parse_options = true;
    for argument in args {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                match option {
                    'a' => options.all = true,
                    'g' => options.external_only = true,
                    'u' => options.undefined_only = true,
                    'n' => options.numeric = true,
                    'p' => options.no_sort = true,
                    'r' => options.reverse = true,
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
    Ok(options)
}

/// Read `data` and return its class plus the selected entries.
pub fn inspect(data: &[u8], options: &Options<'_>) -> Result<(Class, Vec<Entry>), NmError> {
    let object = Object::parse(data).map_err(|_| NmError::Malformed)?;
    let mut entries = Vec::new();
    for symbol in object.symbols() {
        let Some(name) = symbol.name() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let local = symbol.binding() == STB_LOCAL;
        let undefined = symbol.section_index() == SHN_UNDEF;
        if options.undefined_only && !undefined {
            continue;
        }
        if options.external_only && local {
            continue;
        }
        if !options.all && matches!(symbol.kind(), STT_FILE | STT_SECTION) {
            continue;
        }
        entries.push(Entry {
            name: name.to_string(),
            kind: type_letter(&object, &symbol, local, undefined),
            value: symbol.value(),
            size: symbol.size(),
            undefined,
        });
    }
    if !options.no_sort {
        if options.numeric {
            entries.sort_by(|left, right| {
                left.value
                    .cmp(&right.value)
                    .then_with(|| left.name.cmp(&right.name))
            });
        } else {
            entries.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then_with(|| left.value.cmp(&right.value))
            });
        }
    }
    if options.reverse {
        entries.reverse();
    }
    Ok((object.class(), entries))
}

/// Render entries in the `value type name` form used by the host tool.
#[must_use]
pub fn render(entries: &[Entry], class: Class) -> Vec<String> {
    let width = match class {
        Class::Elf32 => 8,
        Class::Elf64 => 16,
    };
    let mut lines = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.undefined {
            lines.push(format!(
                "{:width$} {} {}",
                "",
                entry.kind,
                entry.name,
                width = width
            ));
        } else {
            lines.push(format!(
                "{:0width$x} {} {}",
                entry.value,
                entry.kind,
                entry.name,
                width = width
            ));
        }
    }
    lines
}

fn type_letter(object: &Object<'_>, symbol: &Symbol<'_>, local: bool, undefined: bool) -> char {
    if undefined {
        if symbol.binding() == STB_WEAK {
            return if symbol.kind() == STT_OBJECT {
                'v'
            } else {
                'w'
            };
        }
        return 'U';
    }
    if symbol.section_index() == SHN_ABS {
        return if local { 'a' } else { 'A' };
    }
    if symbol.section_index() == SHN_COMMON {
        return if local { 'c' } else { 'C' };
    }
    if symbol.binding() == STB_WEAK {
        let letter = if symbol.kind() == STT_OBJECT {
            'V'
        } else {
            'W'
        };
        return if local {
            letter.to_ascii_lowercase()
        } else {
            letter
        };
    }
    let letter = object
        .section(usize::from(symbol.section_index()))
        .map_or('N', |section| {
            if section.section_type() == SHT_NOBITS {
                'B'
            } else if section.flags() & SHF_EXECINSTR != 0 {
                'T'
            } else if section.flags() & SHF_WRITE != 0 {
                'D'
            } else if section.flags() & SHF_ALLOC != 0 {
                'R'
            } else {
                'N'
            }
        });
    if local {
        letter.to_ascii_lowercase()
    } else {
        letter
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
