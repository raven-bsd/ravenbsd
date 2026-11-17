// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `times` rendering.
//!
//! The kernel exports no process CPU-time clock, so the target wrapper supplies zero ticks and
//! documents the limit. The renderer is exact POSIX `XmY.ZZs` text.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

/// Usage line.
pub const USAGE: &str = "times";

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// An operand or option was supplied.
    UnexpectedOperand,
}

/// Parse `times` arguments (without `argv[0]`).
pub fn parse_args<'a, I>(args: I) -> Result<(), ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    if args.into_iter().next().is_some() {
        Err(ParseError::UnexpectedOperand)
    } else {
        Ok(())
    }
}

/// Render one `minutes m seconds` value from clock ticks.
pub fn format_one(ticks: u64, ticks_per_second: u64) -> String {
    let hz = ticks_per_second.max(1);
    let total = ticks / hz;
    let minutes = total / 60;
    let seconds = total % 60;
    let hundredths = ticks
        .saturating_mul(100)
        .saturating_div(hz)
        .saturating_sub(total.saturating_mul(100));
    format!("{minutes}m{seconds}.{hundredths:02}s")
}

/// Render the POSIX two-line `times` output.
pub fn render(user: u64, sys: u64, child_user: u64, child_sys: u64, hz: u64) -> String {
    format!(
        "{} {}\n{} {}\n",
        format_one(user, hz),
        format_one(sys, hz),
        format_one(child_user, hz),
        format_one(child_sys, hz)
    )
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
