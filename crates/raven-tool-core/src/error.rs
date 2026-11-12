// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error, usage and exit-code helpers.

use alloc::format;
use alloc::string::String;

use raven_abi::errno::Errno;

/// Successful exit status.
pub const EXIT_OK: i32 = 0;
/// A command failed; the error was printed.
pub const EXIT_ERROR: i32 = 1;
/// The command line was invalid.
pub const EXIT_USAGE: i32 = 2;

/// Format one diagnostic line as `<tool>: <path>: <errno text>`.
pub fn line(tool: &str, path: &str, error: Errno) -> String {
    format!("{tool}: {path}: {}", error.description())
}

/// Format a usage line as `usage: <tool> <syntax>`.
pub fn usage(tool: &str, syntax: &str) -> String {
    format!("usage: {tool} {syntax}")
}
