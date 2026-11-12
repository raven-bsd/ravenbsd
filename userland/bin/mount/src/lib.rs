// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing and `/etc/mounts` formatting for `mount`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// List the mounts.
    List,
    /// Mount `scheme` at `path`.
    Mount {
        /// Scheme name including the trailing colon.
        scheme: &'a str,
        /// Absolute mount point.
        path: &'a str,
    },
    /// Check the mount server against the kernel.
    Selftest,
}

/// The command line was not a valid `mount` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: mount | mount <scheme:> <path> | mount --selftest")
    }
}

/// Parse the arguments after `argv[0]`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Action<'a>, UsageError> {
    let mut args = args;
    match (args.next(), args.next(), args.next()) {
        (None, _, _) => Ok(Action::List),
        (Some("--selftest"), None, None) => Ok(Action::Selftest),
        (Some(scheme), Some(path), None) if valid_scheme(scheme) && valid_path(path) => {
            Ok(Action::Mount { scheme, path })
        }
        _ => Err(UsageError),
    }
}

/// True when `scheme` is a non-empty name followed by `:`.
pub fn valid_scheme(scheme: &str) -> bool {
    let Some(name) = scheme.strip_suffix(':') else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
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

/// Render `/etc/mounts` as `<scheme> on <path>` lines, skipping comments and blank lines.
pub fn format_mounts(text: &str, out: &mut impl fmt::Write) -> fmt::Result {
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(path) = fields.next() else {
            continue;
        };
        let Some(scheme) = fields.next() else {
            continue;
        };
        writeln!(out, "{scheme} on {path}")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
