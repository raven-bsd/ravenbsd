// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing for `umount`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// Unmount this absolute path.
    Path(&'a str),
    /// Check the mount server against the kernel.
    Selftest,
}

/// The command line was not a valid `umount` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: umount <path> | umount --selftest")
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Action<'a>, UsageError> {
    let mut args = args;
    match (args.next(), args.next()) {
        (Some("--selftest"), None) => Ok(Action::Selftest),
        (Some(path), None) if valid_path(path) => Ok(Action::Path(path)),
        _ => Err(UsageError),
    }
}

/// True for a bounded absolute C string without control bytes.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 255
        && path.starts_with('/')
        && !path
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
