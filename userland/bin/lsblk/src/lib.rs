// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX-style block-device listing over the `block:` scheme.
//!
//! The target wrapper enumerates `block:0..block:7`, asks each device for its
//! `BlockInfo` and GPT partition list, and renders the real records. A missing
//! device is skipped; an enumeration error is never hidden.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_proto_block::{BlockInfo, PartitionRecord};

/// Usage line.
pub const USAGE: &str = "lsblk [-n] [device ...]";

/// Format one device and its partitions.
pub fn render(name: &str, info: &BlockInfo, parts: &[PartitionRecord], names: bool) -> String {
    let mut out = String::new();
    if names {
        out.push_str("NAME SIZE SECTORS TYPE\n");
    }
    out.push_str(&format!(
        "{} {} {} disk\n",
        name,
        info.sectors.saturating_mul(u64::from(info.sector_size)),
        info.sectors
    ));
    for part in parts {
        let label = partition_name(part);
        if label.is_empty() {
            out.push_str(&format!(
                "{}p{} {} {} part\n",
                name,
                part.index,
                part.sectors.saturating_mul(u64::from(info.sector_size)),
                part.sectors
            ));
        } else {
            out.push_str(&format!(
                "{}p{} {} {} part {}\n",
                name,
                part.index,
                part.sectors.saturating_mul(u64::from(info.sector_size)),
                part.sectors,
                label
            ));
        }
    }
    out
}

/// Decode the stored GPT name.
pub fn partition_name(part: &PartitionRecord) -> String {
    let length = usize::from(part.name_len).min(part.name.len());
    let bytes = part.name.get(..length).unwrap_or(&[]);
    String::from_utf8_lossy(bytes).into_owned()
}

/// The command line contains an unsupported option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

impl core::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("unsupported lsblk option")
    }
}

/// Parse the optional `-n` flag and explicit device names.
pub fn parse_args<'a, I>(args: I) -> Result<(bool, Vec<&'a str>), ParseError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut no_header = false;
    let mut devices = Vec::new();
    for arg in args {
        if arg == "-n" {
            no_header = true;
        } else if arg.starts_with('-') && arg != "-" {
            return Err(ParseError);
        } else {
            devices.push(arg);
        }
    }
    Ok((no_header, devices))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
