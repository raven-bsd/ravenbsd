// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX pax archive model.
//!
//! The base record is ustar; paths, link paths, sizes and mtimes that do not
//! fit the ustar fields are carried in `x` extended headers using the pax
//! `<length> <keyword>=<value>\n` record format. Compression is not supported.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One 512-byte block.
pub const BLOCK: usize = 512;
/// Name field length.
pub const NAME_LEN: usize = 100;
/// Prefix field length.
pub const PREFIX_LEN: usize = 155;
/// Largest size that fits the ustar 12-byte octal field.
pub const USTAR_MAX_SIZE: u64 = 0o777_7777_7777;

/// Why an archive is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A header/data block is truncated.
    Truncated,
    /// The checksum does not match.
    Checksum,
    /// The ustar magic is missing.
    BadMagic,
    /// A numeric field is not octal.
    BadNumber,
    /// A pax record is malformed.
    BadRecord,
    /// The member type is outside this subset.
    UnsupportedType(u8),
    /// A member path is unsafe for extraction.
    UnsafePath,
}

/// One archive member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Path.
    pub name: String,
    /// Permission bits.
    pub mode: u32,
    /// Owner id.
    pub uid: u32,
    /// Group id.
    pub gid: u32,
    /// Modification time.
    pub mtime: i64,
    /// `0` regular, `5` directory, `2` symbolic link.
    pub kind: u8,
    /// Symlink target.
    pub linkname: String,
    /// Regular-file bytes.
    pub data: Vec<u8>,
}

/// One requested operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Write a new archive.
    Write,
    /// Append to an existing archive.
    Append,
    /// Read and extract.
    Read,
    /// Extract.
    Extract,
    /// List.
    List,
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Requested operation.
    pub operation: Operation,
    /// Archive path.
    pub archive: String,
    /// Verbose list/extract.
    pub verbose: bool,
    /// Preserve modes (the implementation restores modes in extract anyway).
    pub preserve: bool,
    /// Paths or member filters.
    pub operands: Vec<String>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No operation.
    MissingOperation,
    /// More than one operation.
    MultipleOperations,
    /// Missing `-f`.
    MissingArchive,
    /// A write/append operation has no paths.
    MissingPaths,
    /// Compression was requested.
    UnsupportedCompression,
    /// Unknown option.
    UnknownOption,
    /// `-f` has no value.
    MissingFileValue,
}

/// Parse `pax -w|-a|-r|-x|-t [-v] [-p] -f archive [paths...]`.
///
/// # Errors
///
/// Returns [`UsageError`] for a missing/ambiguous operation, compression,
/// an unknown option or missing operands.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut operation = None;
    let mut archive = None;
    let mut verbose = false;
    let mut preserve = false;
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
                    'w' => set_operation(&mut operation, Operation::Write)?,
                    'a' => set_operation(&mut operation, Operation::Append)?,
                    'r' => set_operation(&mut operation, Operation::Read)?,
                    'x' => set_operation(&mut operation, Operation::Extract)?,
                    't' => set_operation(&mut operation, Operation::List)?,
                    'v' => verbose = true,
                    'p' => preserve = true,
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
    if matches!(operation, Operation::Write | Operation::Append) && operands.is_empty() {
        return Err(UsageError::MissingPaths);
    }
    Ok(Options {
        operation,
        archive,
        verbose,
        preserve,
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

/// Encode entries, adding pax extended headers where the ustar fields are too small.
///
/// # Errors
///
/// Returns [`Error::UnsupportedType`] for an unknown member kind.
pub fn encode_archive(entries: &[Entry]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let extended = needs_extended(entry);
        if extended {
            let records = extended_records(entry);
            let header = base_header(
                &format!("PaxHeaders/{index}"),
                "",
                0,
                0,
                0,
                0o644,
                records.len() as u64,
                b'x',
                "",
            )?;
            out.extend_from_slice(&header);
            out.extend_from_slice(&records);
            pad(&mut out, records.len());
        }
        let (base_name, base_prefix) = base_name(entry);
        let size = if entry.kind == b'0' && entry.data.len() as u64 <= USTAR_MAX_SIZE {
            entry.data.len() as u64
        } else {
            0
        };
        let link = if entry.linkname.len() <= NAME_LEN {
            entry.linkname.as_str()
        } else {
            ""
        };
        let header = base_header(
            &base_name,
            &base_prefix,
            entry.uid,
            entry.gid,
            entry.mtime.max(0) as u64,
            entry.mode,
            size,
            entry.kind,
            link,
        )?;
        out.extend_from_slice(&header);
        if entry.kind == b'0' {
            out.extend_from_slice(&entry.data);
            pad(&mut out, entry.data.len());
        }
    }
    out.extend(core::iter::repeat_n(0u8, BLOCK * 2));
    Ok(out)
}

/// Parse a pax archive, applying `x` extended headers to the following member.
///
/// # Errors
///
/// Returns [`Error`] for a truncated/ill-formed block, a bad checksum/magic, a
/// bad pax record or a member type outside this subset.
pub fn parse_archive(bytes: &[u8]) -> Result<Vec<Entry>, Error> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    let mut pending = Extended::default();
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset.checked_add(BLOCK).ok_or(Error::Truncated)?)
            .ok_or(Error::Truncated)?;
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        check_header(header)?;
        if header.get(257..262) != Some(b"ustar") {
            return Err(Error::BadMagic);
        }
        let name = field(header, 0, NAME_LEN);
        let prefix = field(header, 345, PREFIX_LEN);
        let base_name = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let mode = parse_octal(&header[100..108])? as u32;
        let uid = parse_octal(&header[108..116])? as u32;
        let gid = parse_octal(&header[116..124])? as u32;
        let size = parse_octal(&header[124..136])?;
        let mtime = parse_octal(&header[136..148])? as i64;
        let kind = header[156];
        let link = field(header, 157, NAME_LEN);
        let data_start = offset + BLOCK;
        let data_len = if kind == b'x' || kind == b'g' {
            usize::try_from(size).map_err(|_| Error::Truncated)?
        } else {
            usize::try_from(pending.size.unwrap_or(size)).map_err(|_| Error::Truncated)?
        };
        let data_end = data_start.checked_add(data_len).ok_or(Error::Truncated)?;
        let data = bytes.get(data_start..data_end).ok_or(Error::Truncated)?;
        offset = data_start + align(data_len);
        if kind == b'x' {
            pending = Extended::parse(data)?;
            continue;
        }
        if kind == b'g' {
            let global = Extended::parse(data)?;
            pending.merge(global);
            continue;
        }
        let entry = Entry {
            name: pending.path.clone().unwrap_or(base_name),
            mode,
            uid,
            gid,
            mtime: pending.mtime.unwrap_or(mtime),
            kind: match kind {
                0 | b'0' => b'0',
                b'5' => b'5',
                b'2' => b'2',
                other => return Err(Error::UnsupportedType(other)),
            },
            linkname: pending.linkpath.clone().unwrap_or(link),
            data: if kind == b'0' {
                data.to_vec()
            } else {
                Vec::new()
            },
        };
        entries.push(entry);
        pending = Extended::default();
    }
    Ok(entries)
}

/// Whether a path is safe to extract below the current directory.
#[must_use]
pub fn safe_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.split('/').any(|part| part == "..")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Extended {
    path: Option<String>,
    linkpath: Option<String>,
    size: Option<u64>,
    mtime: Option<i64>,
}

impl Extended {
    fn merge(&mut self, other: Self) {
        if other.path.is_some() {
            self.path = other.path;
        }
        if other.linkpath.is_some() {
            self.linkpath = other.linkpath;
        }
        if other.size.is_some() {
            self.size = other.size;
        }
        if other.mtime.is_some() {
            self.mtime = other.mtime;
        }
    }

    fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut extended = Self::default();
        let mut offset = 0usize;
        while offset < data.len() {
            let rest = data.get(offset..).ok_or(Error::BadRecord)?;
            let space = rest
                .iter()
                .position(|byte| *byte == b' ')
                .ok_or(Error::BadRecord)?;
            let length = core::str::from_utf8(rest.get(..space).unwrap_or(&[]))
                .map_err(|_| Error::BadRecord)?
                .parse::<usize>()
                .map_err(|_| Error::BadRecord)?;
            if length <= space + 1
                || offset.checked_add(length).ok_or(Error::BadRecord)? > data.len()
            {
                return Err(Error::BadRecord);
            }
            let record = data.get(offset..offset + length).ok_or(Error::BadRecord)?;
            if record.last() != Some(&b'\n') {
                return Err(Error::BadRecord);
            }
            let body = record
                .get(space + 1..record.len().saturating_sub(1))
                .ok_or(Error::BadRecord)?;
            let equals = body
                .iter()
                .position(|byte| *byte == b'=')
                .ok_or(Error::BadRecord)?;
            let key = core::str::from_utf8(body.get(..equals).unwrap_or(&[]))
                .map_err(|_| Error::BadRecord)?;
            let value = core::str::from_utf8(body.get(equals + 1..).unwrap_or(&[]))
                .map_err(|_| Error::BadRecord)?;
            match key {
                "path" => extended.path = Some(value.to_string()),
                "linkpath" => extended.linkpath = Some(value.to_string()),
                "size" => extended.size = Some(value.parse().map_err(|_| Error::BadRecord)?),
                "mtime" => extended.mtime = Some(value.parse().map_err(|_| Error::BadRecord)?),
                _ => {}
            }
            offset += length;
        }
        Ok(extended)
    }
}

fn needs_extended(entry: &Entry) -> bool {
    !fits_name(&entry.name)
        || entry.linkname.len() > NAME_LEN
        || (entry.kind == b'0' && entry.data.len() as u64 > USTAR_MAX_SIZE)
        || entry.mtime < 0
}

fn extended_records(entry: &Entry) -> Vec<u8> {
    let mut records = Vec::new();
    if !fits_name(&entry.name) {
        records.extend_from_slice(pax_record("path", &entry.name).as_bytes());
    }
    if entry.linkname.len() > NAME_LEN {
        records.extend_from_slice(pax_record("linkpath", &entry.linkname).as_bytes());
    }
    if entry.kind == b'0' && entry.data.len() as u64 > USTAR_MAX_SIZE {
        records.extend_from_slice(pax_record("size", &entry.data.len().to_string()).as_bytes());
    }
    if entry.mtime < 0 {
        records.extend_from_slice(pax_record("mtime", &entry.mtime.to_string()).as_bytes());
    }
    records
}

fn pax_record(key: &str, value: &str) -> String {
    let body = format!(" {key}={value}\n");
    let mut length = body.len() + 1;
    loop {
        let record = format!("{length}{body}");
        if record.len() == length {
            return record;
        }
        length = record.len();
    }
}

fn fits_name(path: &str) -> bool {
    path.len() <= NAME_LEN || split_name(path).is_some()
}

fn split_name(path: &str) -> Option<(&str, &str)> {
    if path.len() <= NAME_LEN {
        return Some((path, ""));
    }
    let bytes = path.as_bytes();
    for index in (0..bytes.len()).rev() {
        if bytes[index] != b'/' {
            continue;
        }
        let prefix = &path[..index];
        let name = &path[index + 1..];
        if prefix.len() <= PREFIX_LEN && !name.is_empty() && name.len() <= NAME_LEN {
            return Some((name, prefix));
        }
    }
    None
}

fn base_name(entry: &Entry) -> (String, String) {
    if let Some((name, prefix)) = split_name(&entry.name) {
        return (name.to_string(), prefix.to_string());
    }
    let base = entry.name.rsplit('/').next().unwrap_or("pax-member");
    let base = if base.is_empty() { "pax-member" } else { base };
    (
        base.get(..base.len().min(NAME_LEN))
            .unwrap_or(base)
            .to_string(),
        String::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn base_header(
    name: &str,
    prefix: &str,
    uid: u32,
    gid: u32,
    mtime: u64,
    mode: u32,
    size: u64,
    kind: u8,
    linkname: &str,
) -> Result<[u8; BLOCK], Error> {
    if !fits_name_parts(name, prefix) {
        return Err(Error::BadNumber);
    }
    let mut header = [0u8; BLOCK];
    put(&mut header[0..NAME_LEN], name);
    put_octal(&mut header[100..108], u64::from(mode & 0o7777));
    put_octal(&mut header[108..116], u64::from(uid));
    put_octal(&mut header[116..124], u64::from(gid));
    put_octal(&mut header[124..136], size);
    put_octal(&mut header[136..148], mtime);
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = kind;
    put(&mut header[157..257], linkname);
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    put(&mut header[345..500], prefix);
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let text = format!("{checksum:06o}");
    header[148..154].copy_from_slice(text.as_bytes());
    header[154] = 0;
    header[155] = b' ';
    Ok(header)
}

fn fits_name_parts(name: &str, prefix: &str) -> bool {
    name.len() <= NAME_LEN && prefix.len() <= PREFIX_LEN
}

fn check_header(header: &[u8]) -> Result<(), Error> {
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
        return Err(Error::Checksum);
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

fn parse_octal(bytes: &[u8]) -> Result<u64, Error> {
    let text = core::str::from_utf8(bytes)
        .map_err(|_| Error::BadNumber)?
        .trim_matches(|byte: char| byte == '\0' || byte == ' ');
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| Error::BadNumber)
}

fn align(length: usize) -> usize {
    length.div_ceil(BLOCK) * BLOCK
}

fn pad(out: &mut Vec<u8>, length: usize) {
    out.extend(core::iter::repeat_n(0u8, align(length) - length));
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
