// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure classification and argument handling for `file`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_proto_fs::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK};

/// One filesystem kind relevant to `file`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A regular file.
    Regular,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// A FIFO.
    Fifo,
    /// A character device.
    Char,
    /// A block device.
    Block,
    /// A socket.
    Socket,
    /// An unknown mode.
    Other,
}

impl Kind {
    /// Map a POSIX mode to a kind.
    #[must_use]
    pub fn from_mode(mode: u32) -> Self {
        match mode & S_IFMT {
            S_IFREG => Self::Regular,
            S_IFDIR => Self::Directory,
            S_IFLNK => Self::Symlink,
            S_IFIFO => Self::Fifo,
            S_IFCHR => Self::Char,
            S_IFBLK => Self::Block,
            S_IFSOCK => Self::Socket,
            _ => Self::Other,
        }
    }
}

/// Parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Omit the path prefix.
    pub brief: bool,
    /// Print MIME types.
    pub mime: bool,
    /// Input paths.
    pub operands: Vec<&'a str>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown short option.
    UnknownOption(char),
}

/// Parse `file [-b] [-i] [--] path ...`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options::default();
    let mut parse_options = true;
    for argument in args {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                match option {
                    'b' => options.brief = true,
                    'i' => options.mime = true,
                    other => return Err(UsageError::UnknownOption(other)),
                }
            }
            continue;
        }
        options.operands.push(argument);
    }
    Ok(options)
}

/// Render one output line.
#[must_use]
pub fn line(path: &str, description: &str, brief: bool) -> String {
    if brief {
        description.to_string()
    } else {
        format!("{path}: {description}")
    }
}

/// Describe a non-regular entry or a regular file sample.
#[must_use]
pub fn describe(kind: Kind, target: Option<&str>, sample: &[u8], mime: bool) -> String {
    if mime {
        return describe_mime(kind, sample).to_string();
    }
    match kind {
        Kind::Directory => "directory".to_string(),
        Kind::Symlink => match target {
            Some(target) => format!("symbolic link to {target}"),
            None => "symbolic link".to_string(),
        },
        Kind::Fifo => "fifo (named pipe)".to_string(),
        Kind::Char => "character special file".to_string(),
        Kind::Block => "block special file".to_string(),
        Kind::Socket => "socket".to_string(),
        Kind::Other => "data".to_string(),
        Kind::Regular => describe_regular(sample),
    }
}

fn describe_regular(sample: &[u8]) -> String {
    if let Some(kind) = elf_kind(sample) {
        return kind;
    }
    if is_text(sample) {
        "ASCII text".to_string()
    } else {
        "data".to_string()
    }
}

fn describe_mime(kind: Kind, sample: &[u8]) -> &'static str {
    match kind {
        Kind::Directory => "inode/directory",
        Kind::Symlink => "inode/symlink",
        Kind::Fifo => "inode/fifo",
        Kind::Char => "inode/chardevice",
        Kind::Block => "inode/blockdevice",
        Kind::Socket => "inode/socket",
        Kind::Other => "application/octet-stream",
        Kind::Regular => {
            if let Some(kind) = elf_mime(sample) {
                kind
            } else if is_text(sample) {
                "text/plain"
            } else {
                "application/octet-stream"
            }
        }
    }
}

fn elf_kind(sample: &[u8]) -> Option<String> {
    if sample.len() < 18 || &sample[..4] != b"\x7fELF" {
        return None;
    }
    let class = match sample[4] {
        1 => "32-bit",
        2 => "64-bit",
        _ => return Some("ELF data".to_string()),
    };
    let data = match sample[5] {
        1 => "LSB",
        2 => "MSB",
        _ => "unknown",
    };
    let e_type = u16::from_le_bytes([sample[16], sample[17]]);
    let kind = match e_type {
        1 => "relocatable",
        2 => "executable",
        3 => "shared object",
        4 => "core file",
        _ => "unknown type",
    };
    Some(format!("ELF {class} {data} {kind}"))
}

fn elf_mime(sample: &[u8]) -> Option<&'static str> {
    if sample.len() < 18 || &sample[..4] != b"\x7fELF" {
        return None;
    }
    Some(match u16::from_le_bytes([sample[16], sample[17]]) {
        1 => "application/x-object",
        2 => "application/x-executable",
        3 => "application/x-sharedlib",
        4 => "application/x-coredump",
        _ => "application/octet-stream",
    })
}

fn is_text(sample: &[u8]) -> bool {
    if sample.contains(&0) {
        return false;
    }
    sample.iter().all(|byte| {
        matches!(*byte, b'\n' | b'\r' | b'\t' | 0x0c | 0x08)
            || (0x20..=0x7e).contains(byte)
            || *byte >= 0x80
    })
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
