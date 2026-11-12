// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and line formatting for `ls`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use raven_tool_core::format::{mode_string, size_string};
use raven_tool_core::{Parsed, UsageError, parse as parse_options};

/// Parse `ls [-a] [-l] [-h] [path ...]`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "alh")
}

/// Format one short-list line; directories get a trailing slash.
pub fn format_short(name: &str, is_dir: bool) -> String {
    if is_dir {
        format!("{name}/")
    } else {
        name.into()
    }
}

/// Format one long-list line: mode, size and name.
pub fn format_long(name: &str, mode: u32, size: u64) -> String {
    format!("{} {:>8} {}", mode_string(mode), size_string(size), name)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
