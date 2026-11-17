// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mailx` front end over the bounded local mail core.

#![cfg_attr(target_os = "none", no_std)]
#![cfg(not(target_os = "none"))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Tool name used in diagnostics and usage text.
pub const TOOL: &str = "mailx";

/// Run the command line and return its exit status.
pub fn run(args: &[String]) -> i32 {
    mail::run(TOOL, args)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
