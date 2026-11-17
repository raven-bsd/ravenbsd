// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and the bounded configuration table for `getconf`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The synopsis printed after `usage: getconf`.
pub const USAGE: &str = "[-a] [name ...]";

/// One known configuration value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// POSIX variable name.
    pub name: &'static str,
    /// Value as the utility prints it.
    pub value: &'static str,
}

/// The values this bounded implementation exposes.
///
/// `ARG_MAX` and `PAGESIZE` mirror `include/limits.h` and `raven_abi::PAGE_SIZE`; the POSIX
/// version mirrors `include/unistd.h`. Unsupported names are rejected rather than invented.
pub const ENTRIES: &[Entry] = &[
    Entry {
        name: "ARG_MAX",
        value: "131072",
    },
    Entry {
        name: "PAGESIZE",
        value: "4096",
    },
    Entry {
        name: "PAGE_SIZE",
        value: "4096",
    },
    Entry {
        name: "_POSIX_VERSION",
        value: "202405",
    },
    Entry {
        name: "_POSIX_ARG_MAX",
        value: "4096",
    },
    Entry {
        name: "_POSIX_CHOWN_RESTRICTED",
        value: "1",
    },
    Entry {
        name: "_POSIX_NAME_MAX",
        value: "255",
    },
    Entry {
        name: "_POSIX_PATH_MAX",
        value: "256",
    },
    Entry {
        name: "_POSIX_PIPE_BUF",
        value: "4096",
    },
    Entry {
        name: "_POSIX_VDISABLE",
        value: "0",
    },
];

/// A `getconf` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Print every supported entry.
    pub all: bool,
    /// Requested variable names, in command-line order.
    pub names: Vec<String>,
}

/// Why a `getconf` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No variable name and no `-a`.
    MissingName,
    /// An unknown option.
    UnknownOption,
}

/// Parse `getconf [-a] [name ...]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut all = false;
    let mut names = Vec::new();
    let mut parsing = true;
    for argument in args {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && (argument == "-a" || argument == "--all") {
            all = true;
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" {
            return Err(UsageError::UnknownOption);
        }
        names.push(argument.to_string());
    }
    if !all && names.is_empty() {
        return Err(UsageError::MissingName);
    }
    Ok(Options { all, names })
}

/// Look up one supported variable.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static str> {
    ENTRIES
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.value)
}

/// Render one `name value` line for `-a`.
#[must_use]
pub fn all_line(entry: &Entry) -> String {
    let mut line = entry.name.to_string();
    line.push(' ');
    line.push_str(entry.value);
    line
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
