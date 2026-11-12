// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and destination handling for `cp`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};

use raven_tool_core::path::move_into_directory;
use raven_tool_core::{Parsed, UsageError, parse as parse_options};

/// Parse `cp [-r] source target`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "r")
}

/// Return the two operands, or `None` when the count is not exactly two.
pub fn pair<'a>(operands: &[&'a str]) -> Option<(&'a str, &'a str)> {
    match operands {
        [source, target] => Some((*source, *target)),
        _ => None,
    }
}

/// Where the source lands when the second operand is (or is not) a directory.
pub fn destination(source: &str, target: &str, target_is_dir: bool) -> String {
    if target_is_dir {
        move_into_directory(source, target)
    } else {
        target.to_string()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
