// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX ustar archive model.
//!
//! The implementation covers regular files, directories and symbolic links with
//! the 100-byte ustar name field plus the 155-byte prefix field. Long-name/PAX
//! records are deliberately rejected instead of being truncated.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One ustar block.
pub const BLOCK: usize = 512;
/// Header name field length.
pub const NAME_LEN: usize = 100;
/// Header prefix field length.
pub const PREFIX_LEN: usize = 155;

/// Why an archive is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TarError {
    /// A header or data block is truncated.
    Truncated,
    /// The checksum does not match the header bytes.
    Checksum,
    /// The ustar magic is missing.
    BadMagic,
    /// A numeric field is not octal.
    BadNumber,
    /// A name does not fit the ustar fields.
    NameTooLong,
    /// The member type is outside this subset.
    UnsupportedType(u8),
    /// A member path is absolute or escapes the extraction root.
    UnsafePath,
}

/// One archive member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Path as stored (directories end with `/`).
    pub name: String,
    /// Permission bits.
    pub mode: u32,
    /// Owner id (written as zero for non-root).
    pub uid: u32,
    /// Group id.
    pub gid: u32,
    /// Modification time.
    pub mtime: i64,
    /// `0` regular, `5` directory, `2` symbolic link.
    pub kind: u8,
    /// Symlink target for kind `2`.
    pub linkname: String,
    /// Regular-file bytes.
    pub data: Vec<u8>,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Archive operation.
    pub operation: Operation,
    /// Archive path.
    pub archive: String,
    /// Verbose listing/extraction.
    pub verbose: bool,
    /// Paths or member filters.
    pub operands: Vec<String>,
}

/// One requested operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Create an archive.
    Create,
    /// List members.
    List,
    /// Extract members.
    Extract,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No operation was supplied.
    MissingOperation,
    /// More than one operation was supplied.
    MultipleOperations,
    /// `-f` is missing.
    MissingArchive,
    /// A create operation has no path operands.
    MissingPaths,
    /// Compression was requested but is not in this ticket.
    UnsupportedCompression,
    /// An unknown option was supplied.
    UnknownOption,
    /// `-f` has no following value.
    MissingFileValue,
}

/// Parse `tar -c|-t|-x [-v] -f archive [paths...]`.
///
/// # Errors
///
/// Returns [`UsageError`] for a missing/ambiguous operation, compression,
/// an unknown option or a missing archive/member operand.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut operation = None;
    let mut archive = None;
    let mut verbose = false;
    let mut operands = Vec::new();
    let mut args = args.into_iter().peekable();
    let mut options = true;
    while let Some(argument) = args.next() {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.starts_with('-') && argument.len() > 1 {
            let mut chars = argument.chars().skip(1).peekable();
            while let Some(flag) = chars.next() {
                match flag {
                    'c' => set_operation(&mut operation, Operation::Create)?,
                    't' => set_operation(&mut operation, Operation::List)?,
                    'x' => set_operation(&mut operation, Operation::Extract)?,
                    'v' => verbose = true,
                    'f' => {
                        let rest: String = chars.collect();
                        if rest.is_empty() {
                            archive = Some(args.next().ok_or(UsageError::MissingFileValue)?);
                        } else {
                            archive = Some(rest);
                        }
                        break;
                    }
                    'z' | 'j' | 'J' => return Err(UsageError::UnsupportedCompression),
                    _ => return Err(UsageError::UnknownOption),
                }
            }
            continue;
        }
        operands.push(argument);
    }
    let operation = operation.ok_or(UsageError::MissingOperation)?;
    let archive = archive.ok_or(UsageError::MissingArchive)?;
    if operation == Operation::Create && operands.is_empty() {
        return Err(UsageError::MissingPaths);
    }
    Ok(Options {
        operation,
        archive,
        verbose,
        operands,
    })
}

fn set_operation(slot: &mut Option<Operation>, value: Operation) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError::MultipleOperations);
    }
    *slot = Some(value);
    Ok(())
}

/// Parse a complete ustar archive.
///
/// # Errors
///
/// Returns [`TarError`] for a truncated block, bad checksum/magic, a numeric
/// field that is not octal, or a member type outside this subset.
pub fn parse_archive(bytes: &[u8]) -> Result<Vec<Entry>, TarError> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset.checked_add(BLOCK).ok_or(TarError::Truncated)?)
            .ok_or(TarError::Truncated)?;
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        check_header(header)?;
        if header.get(257..262) != Some(b"ustar") {
            return Err(TarError::BadMagic);
        }
        let name = field(header, 0, NAME_LEN);
        let prefix = field(header, 345, PREFIX_LEN);
        let name = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if name.is_empty() {
            return Err(TarError::UnsafePath);
        }
        let mode = parse_octal(&header[100..108])? as u32;
        let uid = parse_octal(&header[108..116])? as u32;
        let gid = parse_octal(&header[116..124])? as u32;
        let size = parse_octal(&header[124..136])?;
        let mtime = parse_octal(&header[136..148])? as i64;
        let kind = match header[156] {
            0 | b'0' => b'0',
            b'5' => b'5',
            b'2' => b'2',
            other => return Err(TarError::UnsupportedType(other)),
        };
        let linkname = field(header, 157, 100);
        let start = offset + BLOCK;
        let end = start
            .checked_add(size as usize)
            .ok_or(TarError::Truncated)?;
        let data = bytes.get(start..end).ok_or(TarError::Truncated)?.to_vec();
        offset = start
            .checked_add(align(size as usize))
            .ok_or(TarError::Truncated)?;
        entries.push(Entry {
            name,
            mode: mode & 0o7777,
            uid,
            gid,
            mtime,
            kind,
            linkname,
            data,
        });
    }
    Ok(entries)
}

/// Encode entries as a ustar archive.
///
/// # Errors
///
/// Returns [`TarError::NameTooLong`] when a path cannot be split into the
/// ustar name/prefix fields, or [`TarError::UnsupportedType`] for an unknown
/// member kind.
pub fn encode_archive(entries: &[Entry]) -> Result<Vec<u8>, TarError> {
    let mut out = Vec::new();
    for entry in entries {
        let (name, prefix) = split_name(&entry.name)?;
        let mut header = [0u8; BLOCK];
        put(&mut header[0..NAME_LEN], name);
        put_octal(&mut header[100..108], u64::from(entry.mode & 0o7777));
        put_octal(&mut header[108..116], u64::from(entry.uid));
        put_octal(&mut header[116..124], u64::from(entry.gid));
        put_octal(
            &mut header[124..136],
            if entry.kind == b'0' {
                entry.data.len() as u64
            } else {
                0
            },
        );
        put_octal(&mut header[136..148], entry.mtime.max(0) as u64);
        for byte in &mut header[148..156] {
            *byte = b' ';
        }
        header[156] = match entry.kind {
            b'0' => b'0',
            b'5' => b'5',
            b'2' => b'2',
            other => return Err(TarError::UnsupportedType(other)),
        };
        put(&mut header[157..257], &entry.linkname);
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        put(&mut header[345..500], prefix);
        let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        let text = format!("{checksum:06o}");
        header[148..154].copy_from_slice(text.as_bytes());
        header[154] = 0;
        header[155] = b' ';
        out.extend_from_slice(&header);
        if entry.kind == b'0' {
            out.extend_from_slice(&entry.data);
            let padding = align(entry.data.len()) - entry.data.len();
            out.extend(core::iter::repeat_n(0u8, padding));
        }
    }
    out.extend(core::iter::repeat_n(0u8, BLOCK * 2));
    Ok(out)
}

/// Split a path into the ustar name and prefix fields.
///
/// # Errors
///
/// Returns [`TarError::NameTooLong`] when no slash produces a fit.
pub fn split_name(path: &str) -> Result<(&str, &str), TarError> {
    if path.len() <= NAME_LEN {
        return Ok((path, ""));
    }
    let bytes = path.as_bytes();
    for index in (0..bytes.len()).rev() {
        if bytes[index] != b'/' {
            continue;
        }
        let (prefix, name) = (&path[..index], &path[index + 1..]);
        if prefix.len() <= PREFIX_LEN && !name.is_empty() && name.len() <= NAME_LEN {
            return Ok((name, prefix));
        }
    }
    Err(TarError::NameTooLong)
}

/// Whether a path is safe to extract below the current directory.
#[must_use]
pub fn safe_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.split('/').any(|part| part == "..")
}

fn check_header(header: &[u8]) -> Result<(), TarError> {
    let stored = parse_octal(&header[148..156])?;
    let mut checksum = 0u64;
    for (index, byte) in header.iter().enumerate() {
        checksum += u64::from(if (148..156).contains(&index) {
            b' '
        } else {
            *byte
        });
    }
    if checksum != stored {
        return Err(TarError::Checksum);
    }
    Ok(())
}

fn field(header: &[u8], offset: usize, length: usize) -> String {
    let bytes = header.get(offset..offset + length).unwrap_or(&[]);
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(bytes.get(..end).unwrap_or(&[])).to_string()
}

fn put(field: &mut [u8], value: &str) {
    field.fill(0);
    let bytes = value.as_bytes();
    let count = bytes.len().min(field.len());
    field[..count].copy_from_slice(&bytes[..count]);
}

fn put_octal(field: &mut [u8], value: u64) {
    field.fill(0);
    let width = field.len().saturating_sub(1);
    let text = format!("{value:0width$o}");
    let bytes = text.as_bytes();
    let count = bytes.len().min(width);
    field[..count].copy_from_slice(&bytes[..count]);
}

fn parse_octal(bytes: &[u8]) -> Result<u64, TarError> {
    let text = core::str::from_utf8(bytes)
        .map_err(|_| TarError::BadNumber)?
        .trim_matches(|byte: char| byte == '\0' || byte == ' ');
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| TarError::BadNumber)
}

fn align(length: usize) -> usize {
    length.div_ceil(BLOCK) * BLOCK
}

/// A display name for a listing.
#[must_use]
pub fn display_name(entry: &Entry) -> &str {
    &entry.name
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
