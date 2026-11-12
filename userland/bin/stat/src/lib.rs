// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument and record formatting for `stat`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use raven_proto_fs::Stat;
use raven_tool_core::format::{mode_string, size_string};
use raven_tool_core::{Parsed, UsageError, parse as parse_options};

/// Parse `stat path ...`; `stat` has no options besides `--`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Parsed<'a>, UsageError> {
    parse_options(args, "")
}

/// Render the K60 `Stat` fields for one path.
pub fn format_stat(path: &str, stat: &Stat) -> String {
    format!(
        "File: {path}\n  Size: {size}\tBlocks: {blocks}\tIO Block: 4096\n  Inode: {ino}\tLinks: {nlink}\n  Mode: {mode} ({mode_octal:o})\n  Uid: {uid}\tGid: {gid}\nAccess: {atime}\nModify: {mtime}\nChange: {ctime}",
        size = size_string(stat.size),
        blocks = stat.blocks,
        ino = stat.ino,
        nlink = stat.nlink,
        mode = mode_string(stat.mode),
        mode_octal = stat.mode,
        uid = stat.uid,
        gid = stat.gid,
        atime = stat.atime,
        mtime = stat.mtime,
        ctime = stat.ctime,
    )
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
