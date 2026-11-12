// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure removal decisions for `rm`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use raven_tool_core::{Errno, Parsed, UsageError, parse as parse_options};

/// What to do with one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Remove one file.
    Unlink,
    /// Remove one directory after its children.
    RemoveDir,
    /// Silently accept a missing path (`-f`).
    Ignore,
}

/// Parse `rm [-r] [-f] [-i] path ...`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "rfi")
}

/// Choose the removal action for a stat result.
pub fn action(is_dir: bool, recursive: bool) -> Result<Action, Errno> {
    if is_dir && !recursive {
        Err(Errno::EISDIR)
    } else if is_dir {
        Ok(Action::RemoveDir)
    } else {
        Ok(Action::Unlink)
    }
}

/// Handle a missing operand according to `-f`.
pub fn missing(force: bool) -> Result<Action, Errno> {
    if force {
        Ok(Action::Ignore)
    } else {
        Err(Errno::ENOENT)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
