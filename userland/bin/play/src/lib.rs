// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing and report formatting for `play`.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::{String, ToString};

/// Parsed `play` options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// WAV path.
    pub path: String,
}

/// Parse the operands (without `argv[0]`).
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut path = None;
    for arg in args {
        if arg == "--help" || arg == "-h" {
            return Err("usage: play <file.wav>".to_string());
        }
        if arg.starts_with('-') {
            return Err(alloc::format!("play: unknown option `{arg}`"));
        }
        if path.is_some() {
            return Err("usage: play <file.wav>".to_string());
        }
        path = Some(arg.clone());
    }
    let Some(path) = path else {
        return Err("usage: play <file.wav>".to_string());
    };
    Ok(Options { path })
}

/// Human summary of a parsed WAV.
#[must_use]
pub fn summary(frames: usize, rate: u32, channels: u16) -> String {
    alloc::format!(
        "play: ok {frames} frames, {rate} Hz, {channels} channel{}",
        if channels == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
#[path = "play_ut.rs"]
mod tests;
