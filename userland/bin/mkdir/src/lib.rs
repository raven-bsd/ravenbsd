// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and parent-chain handling for `mkdir`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use raven_tool_core::{Parsed, UsageError, parse as parse_options};

/// Parse `mkdir [-p] directory ...`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "p")
}

/// Every absolute prefix of `path`, from the first component to the full path.
///
/// `parents("/a/b")` returns `["/a", "/a/b"]`; `/` itself is not included because it always
/// exists.
pub fn parents(path: &str) -> Vec<String> {
    let mut result = Vec::new();
    if !path.starts_with('/') {
        return result;
    }
    let mut current = String::new();
    for component in path.split('/').filter(|component| !component.is_empty()) {
        current.push('/');
        current.push_str(component);
        result.push(current.clone());
    }
    result
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
