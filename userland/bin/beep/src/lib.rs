// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing and report formatting for `beep`.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::{String, ToString};

/// The default frequency in Hz.
pub const DEFAULT_HZ: u32 = 440;
/// The default duration in milliseconds.
pub const DEFAULT_MS: u32 = 1_000;

/// Parsed `beep` options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// Tone frequency in Hz.
    pub frequency_hz: u32,
    /// Tone duration in milliseconds.
    pub duration_ms: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            frequency_hz: DEFAULT_HZ,
            duration_ms: DEFAULT_MS,
        }
    }
}

/// Parse the operands (without `argv[0]`).
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Err("usage: beep [-f HZ] [-d MS]".to_string()),
            "--frequency" | "-f" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("beep: --frequency needs a value".to_string());
                };
                options.frequency_hz = value
                    .parse::<u32>()
                    .map_err(|_| alloc::format!("beep: bad frequency `{value}`"))?;
            }
            "--duration-ms" | "-d" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("beep: --duration-ms needs a value".to_string());
                };
                options.duration_ms = value
                    .parse::<u32>()
                    .map_err(|_| alloc::format!("beep: bad duration `{value}`"))?;
            }
            other => return Err(alloc::format!("beep: unexpected argument `{other}`")),
        }
        index += 1;
    }
    if options.frequency_hz == 0 || options.frequency_hz > 20_000 {
        return Err("beep: frequency must be 1..=20000 Hz".to_string());
    }
    if options.duration_ms == 0 || options.duration_ms > 60_000 {
        return Err("beep: duration must be 1..=60000 ms".to_string());
    }
    Ok(options)
}

/// Human summary of one tone.
#[must_use]
pub fn summary(options: &Options) -> String {
    alloc::format!(
        "beep: ok {} Hz {} ms",
        options.frequency_hz,
        options.duration_ms
    )
}

#[cfg(test)]
#[path = "beep_ut.rs"]
mod tests;
