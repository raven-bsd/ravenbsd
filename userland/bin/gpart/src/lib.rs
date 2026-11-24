// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `gpart show`: render the real GPT partition list from the `block:` protocol.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use raven_proto_block::{BlockInfo, PartitionRecord};

/// Usage line.
pub const USAGE: &str = "gpart show [block:0]";

/// A `gpart` argument error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpartError {
    /// The first argument is neither `show` nor absent.
    UnknownCommand,
    /// More than one argument follows `show`.
    TooManyArguments,
}

/// Render a GPT geometry and partition table.
pub fn show(name: &str, info: &BlockInfo, parts: &[PartitionRecord]) -> String {
    let mut out = format!(
        "Geom name: {name}\nMediasize: {}\nSectorsize: {}\nScheme: GPT\n",
        info.sectors.saturating_mul(u64::from(info.sector_size)),
        info.sector_size
    );
    out.push_str("  start  size  index  name\n");
    for part in parts {
        let length = usize::from(part.name_len).min(part.name.len());
        let bytes = part.name.get(..length).unwrap_or(&[]);
        let label = String::from_utf8_lossy(bytes);
        out.push_str(&format!(
            "{:>7} {:>5} {:>7}  {}\n",
            part.start_lba, part.sectors, part.index, label
        ));
    }
    out
}

/// Parse `show [device]`.
pub fn parse_args<'a, I>(args: I) -> Result<Option<&'a str>, GpartError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut values = args.into_iter();
    match values.next() {
        Some("show") | None => {}
        Some(_) => return Err(GpartError::UnknownCommand),
    }
    let device = values.next();
    if values.next().is_some() {
        return Err(GpartError::TooManyArguments);
    }
    Ok(device)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
