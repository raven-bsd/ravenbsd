// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing and report formatting for `mixer`.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::{String, ToString};

/// One mixer operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    /// Set the left and right volume to a percentage (0..=100).
    Volume(u32),
    /// Set both channels to silence.
    Mute,
    /// Print the stream status.
    Status,
}

/// Parse the operands (without `argv[0]`).
pub fn parse_args(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => Err("usage: mixer volume PERCENT | mute | status".to_string()),
        Some("volume") => {
            let Some(value) = args.get(1) else {
                return Err("mixer: volume needs a percentage".to_string());
            };
            if args.len() != 2 {
                return Err("usage: mixer volume PERCENT | mute | status".to_string());
            }
            let percent = value
                .trim_end_matches('%')
                .parse::<u32>()
                .map_err(|_| alloc::format!("mixer: bad percentage `{value}`"))?;
            if percent > 100 {
                return Err("mixer: volume must be 0..=100".to_string());
            }
            Ok(Command::Volume(percent))
        }
        Some("mute") => {
            if args.len() != 1 {
                return Err("usage: mixer volume PERCENT | mute | status".to_string());
            }
            Ok(Command::Mute)
        }
        Some("status") => {
            if args.len() != 1 {
                return Err("usage: mixer volume PERCENT | mute | status".to_string());
            }
            Ok(Command::Status)
        }
        Some(other) => Err(alloc::format!("mixer: unknown command `{other}`")),
        None => Err("usage: mixer volume PERCENT | mute | status".to_string()),
    }
}

/// Q15 gain for a percentage.
#[must_use]
pub fn percent_to_gain(percent: u32) -> u16 {
    let value = percent.min(100).saturating_mul(32_768) / 100;
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// A human volume line.
#[must_use]
pub fn volume_line(percent: u32) -> String {
    let gain = percent_to_gain(percent);
    alloc::format!("mixer: volume {percent}% left={gain} right={gain}")
}

/// A human mute line.
#[must_use]
pub fn mute_line() -> String {
    "mixer: muted left=0 right=0".to_string()
}

/// A human status line.
#[must_use]
pub fn status_line(position: u64, underruns: u32, state: &str) -> String {
    alloc::format!("mixer: status position={position} underruns={underruns} {state}")
}

#[cfg(test)]
#[path = "mixer_ut.rs"]
mod tests;
