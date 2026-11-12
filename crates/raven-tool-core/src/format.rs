// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Mode, size and capacity formatting for `ls`, `stat` and `df`.

use alloc::format;
use alloc::string::{String, ToString};

use raven_proto_fs::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFSOCK};

/// Render the ten-character `ls -l` mode string (`drwxr-xr-x`).
pub fn mode_string(mode: u32) -> String {
    let kind = match mode & S_IFMT {
        S_IFDIR => 'd',
        S_IFLNK => 'l',
        S_IFCHR => 'c',
        S_IFBLK => 'b',
        S_IFIFO => 'p',
        S_IFSOCK => 's',
        _ => '-',
    };
    let mut rights = ['-'; 9];
    let bits = [
        0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
    ];
    for (index, bit) in bits.iter().enumerate() {
        if mode & bit != 0 {
            rights[index] = match index % 3 {
                0 => 'r',
                1 => 'w',
                _ => 'x',
            };
        }
    }
    if mode & 0o4000 != 0 {
        rights[2] = if rights[2] == 'x' { 's' } else { 'S' };
    }
    if mode & 0o2000 != 0 {
        rights[5] = if rights[5] == 'x' { 's' } else { 'S' };
    }
    if mode & 0o1000 != 0 {
        rights[8] = if rights[8] == 'x' { 't' } else { 'T' };
    }
    let mut text = String::with_capacity(10);
    text.push(kind);
    for right in rights {
        text.push(right);
    }
    text
}

/// Render a byte size as decimal bytes (the traditional `ls -l` field).
pub fn size_string(bytes: u64) -> String {
    format!("{bytes}")
}

/// Render `blocks` filesystem blocks of `block_size` bytes as 1 KiB units.
pub fn blocks_string(blocks: u64, block_size: u32) -> String {
    let bytes = blocks.saturating_mul(u64::from(block_size));
    format!("{}", bytes / 1024)
}

/// Render the used percentage of `total` blocks, or `0%` for an empty filesystem.
pub fn capacity(used: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    let percent = (u128::from(used) * 100) / u128::from(total);
    format!("{percent}%")
}
