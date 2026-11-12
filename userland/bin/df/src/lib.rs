// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and record formatting for `df`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use raven_proto_fs::StatFs;
use raven_tool_core::format::{blocks_string, capacity};
use raven_tool_core::{Parsed, UsageError, parse as parse_options};

/// Parse `df`; no options or operands are accepted.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "")
}

/// Render the root filesystem statistics in 1 KiB blocks.
pub fn format_df(statfs: &StatFs) -> String {
    let used = statfs.blocks.saturating_sub(statfs.free_blocks);
    format!(
        "Filesystem 1K-blocks Used Available Capacity Mounted on\n/ {} {} {} {} /",
        blocks_string(statfs.blocks, statfs.block_size),
        blocks_string(used, statfs.block_size),
        blocks_string(statfs.free_blocks, statfs.block_size),
        capacity(used, statfs.blocks),
    )
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
