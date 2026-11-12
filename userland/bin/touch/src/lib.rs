// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument handling for `touch`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use raven_tool_core::{Parsed, UsageError, parse as parse_options};

/// Parse `touch file ...`; `touch` has no options besides `--`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
