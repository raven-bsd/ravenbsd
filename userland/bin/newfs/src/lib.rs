// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `newfs` command line and geometry validation.
//!
//! The on-disk work is `ufs_core::mkfs::format`; this module only owns the
//! POSIX/NetBSD-style options, the UFS2 default and the size suffix parser.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;

/// Usage line.
pub const USAGE: &str = "newfs [-O 1|2] [-b block] [-f frag] [-s size] [-L label] [block:0]";

/// Argument errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Unknown option.
    UnknownOption,
    /// Option without a value.
    MissingValue,
    /// Value is not a number/size.
    BadValue,
    /// UFS version must be 1 or 2.
    BadVersion,
    /// Block/fragment geometry is invalid.
    BadGeometry,
}

/// Parsed options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Target device (default `block:0`).
    pub device: &'a str,
    /// UFS version.
    pub version: u8,
    /// Block size.
    pub block_size: u32,
    /// Fragment size.
    pub fragment_size: u32,
    /// Explicit size in bytes (zero uses the device size).
    pub size: u64,
    /// Volume label.
    pub label: &'a str,
}

/// Parse `newfs` arguments (without `argv[0]`).
pub fn parse_args<'a, I>(args: I) -> Result<Options<'a>, ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut version = 2u8;
    let mut block_size = 4096u32;
    let mut fragment_size = 512u32;
    let mut size = 0u64;
    let mut label = "";
    let mut device = "block:0";
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg {
            "-O" => {
                version = iter
                    .next()
                    .ok_or(ParseError::MissingValue)?
                    .parse()
                    .map_err(|_| ParseError::BadValue)?;
            }
            "-b" => {
                block_size = iter
                    .next()
                    .ok_or(ParseError::MissingValue)?
                    .parse()
                    .map_err(|_| ParseError::BadValue)?;
            }
            "-f" => {
                fragment_size = iter
                    .next()
                    .ok_or(ParseError::MissingValue)?
                    .parse()
                    .map_err(|_| ParseError::BadValue)?;
            }
            "-s" => {
                size = parse_size(iter.next().ok_or(ParseError::MissingValue)?)?;
            }
            "-L" => {
                label = iter.next().ok_or(ParseError::MissingValue)?;
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(ParseError::UnknownOption);
            }
            other => device = other,
        }
    }
    if !matches!(version, 1 | 2) {
        return Err(ParseError::BadVersion);
    }
    if !(4096..=65536).contains(&block_size) || !block_size.is_power_of_two() {
        return Err(ParseError::BadGeometry);
    }
    if !(512..=block_size).contains(&fragment_size) || !fragment_size.is_power_of_two() {
        return Err(ParseError::BadGeometry);
    }
    if size != 0 && !size.is_multiple_of(u64::from(block_size)) {
        return Err(ParseError::BadGeometry);
    }
    Ok(Options {
        device,
        version,
        block_size,
        fragment_size,
        size,
        label,
    })
}

fn parse_size(text: &str) -> Result<u64, ParseError> {
    let (digits, multiplier) = match text.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&text[..text.len() - 1], 1024u64),
        Some(b'm' | b'M') => (&text[..text.len() - 1], 1024 * 1024),
        Some(b'g' | b'G') => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        _ => (text, 1),
    };
    digits
        .parse::<u64>()
        .ok()
        .and_then(|value| value.checked_mul(multiplier))
        .ok_or(ParseError::BadValue)
}

/// Render a one-line summary for the target binary.
pub fn summary(options: &Options<'_>) -> String {
    alloc::format!(
        "newfs: {} UFS{} block={} frag={} size={} label={}",
        options.device,
        options.version,
        options.block_size,
        options.fragment_size,
        options.size,
        options.label
    )
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
