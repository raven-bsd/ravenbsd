// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `ar` archive creation, listing and extraction.
//!
//! The archive is the portable `!<arch>\n` format with 60-byte member headers.
//! Short names use the POSIX `name/` field; longer names use the `#1/<len>`
//! extension. A `/` symbol-table member is generated for ELF object members.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Archive global magic.
pub const MAGIC: &[u8; 8] = b"!<arch>\n";
/// Member header length.
pub const HEADER_LEN: usize = 60;

/// Why an archive or argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArError {
    /// The archive is shorter than the magic or a header.
    Truncated,
    /// The global magic is missing.
    BadMagic,
    /// The member trailer is not `` `\n ``.
    BadHeader,
    /// A numeric header field is not a valid number.
    BadNumber,
    /// A member name is empty or malformed.
    BadName,
    /// The long-name form is malformed.
    BadLongName,
    /// The operation is not implemented.
    Unsupported,
}

/// One archive member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Member name without a trailing slash.
    pub name: String,
    /// Modification time in seconds.
    pub mtime: i64,
    /// User id.
    pub uid: u32,
    /// Group id.
    pub gid: u32,
    /// Permission bits.
    pub mode: u32,
    /// Member bytes.
    pub data: Vec<u8>,
}

/// One requested operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Replace or insert members.
    Replace,
    /// List member names.
    List,
    /// Extract members.
    Extract,
    /// Rewrite the symbol table.
    Symbols,
}

/// Parsed `ar` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Requested operation.
    pub operation: Operation,
    /// Verbose output.
    pub verbose: bool,
    /// Force the symbol table even with no ELF symbols.
    pub symbols: bool,
    /// Archive path.
    pub archive: &'a str,
    /// Member operands.
    pub operands: Vec<&'a str>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No operation was supplied.
    MissingOperation,
    /// More than one operation was supplied.
    MultipleOperations,
    /// An unknown option was supplied.
    UnknownOption,
    /// The archive operand is missing.
    MissingArchive,
    /// `r` has no member operands.
    MissingMembers,
    /// `s` takes no member operands.
    ExtraOperands,
}

/// Parse `ar [r|t|x|s][vc] archive [members...]`.
///
/// # Errors
///
/// Returns [`UsageError`] for a missing/ambiguous operation, an unknown option or
/// a missing/extra operand.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err(UsageError::MissingOperation);
    };
    let flags = first.strip_prefix('-').unwrap_or(first);
    if flags.is_empty() {
        return Err(UsageError::MissingOperation);
    }
    let mut operation = None;
    let mut verbose = false;
    let mut symbols = false;
    for flag in flags.chars() {
        match flag {
            'r' => set_operation(&mut operation, Operation::Replace)?,
            't' => set_operation(&mut operation, Operation::List)?,
            'x' => set_operation(&mut operation, Operation::Extract)?,
            's' => {
                if operation.is_none() {
                    operation = Some(Operation::Symbols);
                } else {
                    symbols = true;
                }
            }
            'v' => verbose = true,
            'c' => {}
            _ => return Err(UsageError::UnknownOption),
        }
    }
    let operation = operation.ok_or(UsageError::MissingOperation)?;
    let archive = args.next().ok_or(UsageError::MissingArchive)?;
    let rest: Vec<&str> = args.collect();
    if operation == Operation::Replace && rest.is_empty() {
        return Err(UsageError::MissingMembers);
    }
    if operation == Operation::Symbols && !rest.is_empty() {
        return Err(UsageError::ExtraOperands);
    }
    Ok(Options {
        operation,
        verbose,
        symbols,
        archive,
        operands: rest,
    })
}

fn set_operation(slot: &mut Option<Operation>, value: Operation) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError::MultipleOperations);
    }
    *slot = Some(value);
    Ok(())
}

/// Parse one archive. Symbol-table and long-name pseudo-members are consumed
/// but not returned as ordinary members.
///
/// # Errors
///
/// Returns [`ArError`] for a truncated archive, bad magic, bad header or a bad
/// numeric/name field.
pub fn parse_archive(bytes: &[u8]) -> Result<Vec<Member>, ArError> {
    if bytes.len() < MAGIC.len() {
        return Err(ArError::Truncated);
    }
    if bytes.get(..MAGIC.len()) != Some(MAGIC) {
        return Err(ArError::BadMagic);
    }
    let mut members = Vec::new();
    let mut long_names: Vec<u8> = Vec::new();
    let mut offset = MAGIC.len();
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset.checked_add(HEADER_LEN).ok_or(ArError::Truncated)?)
            .ok_or(ArError::Truncated)?;
        if header.get(58..60) != Some(b"`\n") {
            return Err(ArError::BadHeader);
        }
        let raw_name = field(&header[0..16]);
        let size = parse_decimal(&header[48..58])? as usize;
        let mode = parse_octal(&header[40..48])?;
        let mtime = parse_signed_decimal(&header[16..28])?;
        let uid = u32::try_from(parse_decimal(&header[28..34])?).map_err(|_| ArError::BadNumber)?;
        let gid = u32::try_from(parse_decimal(&header[34..40])?).map_err(|_| ArError::BadNumber)?;
        let start = offset + HEADER_LEN;
        let end = start.checked_add(size).ok_or(ArError::Truncated)?;
        let mut data = bytes.get(start..end).ok_or(ArError::Truncated)?;
        offset = end + (size & 1);

        if raw_name == "/" {
            continue;
        }
        if raw_name == "//" {
            long_names = data.to_vec();
            continue;
        }
        let name = if let Some(length) = raw_name.strip_prefix("#1/") {
            let length = parse_decimal(length.as_bytes())? as usize;
            let name = data.get(..length).ok_or(ArError::BadLongName)?;
            data = data.get(length..).ok_or(ArError::BadLongName)?;
            String::from_utf8_lossy(name).to_string()
        } else if let Some(index) = raw_name.strip_prefix('/') {
            let index = parse_decimal(index.as_bytes())? as usize;
            let tail = long_names.get(index..).ok_or(ArError::BadLongName)?;
            let length = tail
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or(ArError::BadLongName)?;
            let name = tail.get(..length).ok_or(ArError::BadLongName)?;
            String::from_utf8_lossy(name.strip_suffix(b"/").unwrap_or(name)).to_string()
        } else {
            raw_name.strip_suffix('/').unwrap_or(&raw_name).to_string()
        };
        if name.is_empty() {
            return Err(ArError::BadName);
        }
        members.push(Member {
            name,
            mtime,
            uid,
            gid,
            mode,
            data: data.to_vec(),
        });
    }
    Ok(members)
}

/// Encode one archive, optionally adding a `/` symbol table for ELF members.
#[must_use]
pub fn write_archive(members: &[Member], with_symbols: bool) -> Vec<u8> {
    let encoded: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|member| {
            let short = member.name.len() <= 15
                && !member.name.contains('/')
                && !member.name.contains('\n');
            if short {
                let mut payload = member.data.clone();
                payload.shrink_to_fit();
                (member.name.as_str(), payload)
            } else {
                (member.name.as_str(), member.data.clone())
            }
        })
        .collect();
    let symbols: Vec<(usize, String)> = members
        .iter()
        .enumerate()
        .flat_map(|(index, member)| {
            elf_symbols(&member.data)
                .into_iter()
                .map(move |symbol| (index, symbol))
        })
        .collect();
    let symbol_table = with_symbols || !symbols.is_empty();
    let symbol_data_len = if symbol_table {
        4 + symbols.len() * 4
            + symbols
                .iter()
                .map(|(_, name)| name.len() + 1)
                .sum::<usize>()
    } else {
        0
    };
    let symbol_total = if symbol_table {
        HEADER_LEN + align2(symbol_data_len)
    } else {
        0
    };
    let mut offsets = Vec::with_capacity(members.len());
    let mut position = MAGIC.len() + symbol_total;
    for (name, payload) in &encoded {
        let long = name.len() > 15 || name.contains('/') || name.contains('\n');
        let encoded_len = payload.len() + if long { name.len() } else { 0 };
        offsets.push(position);
        position += HEADER_LEN + align2(encoded_len);
    }
    let symbol_data = if symbol_table {
        let mut data = Vec::with_capacity(symbol_data_len);
        data.extend_from_slice(&(symbols.len() as u32).to_be_bytes());
        for (member, _) in &symbols {
            data.extend_from_slice(
                &(offsets.get(*member).copied().unwrap_or(0) as u32).to_be_bytes(),
            );
        }
        for (_, name) in &symbols {
            data.extend_from_slice(name.as_bytes());
            data.push(0);
        }
        data
    } else {
        Vec::new()
    };

    let mut out = Vec::with_capacity(position);
    out.extend_from_slice(MAGIC);
    if symbol_table {
        write_header(&mut out, "/", 0, 0, 0, 0, symbol_data.len());
        out.extend_from_slice(&symbol_data);
        pad(&mut out);
    }
    for (member, (name, payload)) in members.iter().zip(encoded.iter()) {
        let short = name.len() <= 15 && !name.contains('/') && !name.contains('\n');
        if short {
            write_header(
                &mut out,
                &format!("{name}/"),
                member.mtime,
                member.uid,
                member.gid,
                member.mode,
                payload.len(),
            );
        } else {
            write_header(
                &mut out,
                &format!("#1/{}", name.len()),
                member.mtime,
                member.uid,
                member.gid,
                member.mode,
                payload.len() + name.len(),
            );
            out.extend_from_slice(name.as_bytes());
        }
        out.extend_from_slice(payload);
        pad(&mut out);
    }
    out
}

fn write_header(
    out: &mut Vec<u8>,
    name: &str,
    mtime: i64,
    uid: u32,
    gid: u32,
    mode: u32,
    size: usize,
) {
    let mut header = [b' '; HEADER_LEN];
    put(&mut header[0..16], name);
    put(&mut header[16..28], &mtime.to_string());
    put(&mut header[28..34], &uid.to_string());
    put(&mut header[34..40], &gid.to_string());
    put(&mut header[40..48], &format!("{:o}", mode & 0o7777));
    put(&mut header[48..58], &size.to_string());
    header[58] = b'`';
    header[59] = b'\n';
    out.extend_from_slice(&header);
}

fn put(field: &mut [u8], value: &str) {
    for byte in field.iter_mut() {
        *byte = b' ';
    }
    let bytes = value.as_bytes();
    let count = bytes.len().min(field.len());
    field[..count].copy_from_slice(&bytes[..count]);
}

fn pad(out: &mut Vec<u8>) {
    if out.len() % 2 != 0 {
        out.push(b'\n');
    }
}

fn align2(length: usize) -> usize {
    length + (length & 1)
}

fn field(bytes: &[u8]) -> String {
    let text = core::str::from_utf8(bytes).unwrap_or("");
    text.trim().to_string()
}

fn parse_decimal(bytes: &[u8]) -> Result<u64, ArError> {
    let text = core::str::from_utf8(bytes)
        .map_err(|_| ArError::BadNumber)?
        .trim();
    if text.is_empty() {
        return Ok(0);
    }
    text.parse().map_err(|_| ArError::BadNumber)
}

fn parse_signed_decimal(bytes: &[u8]) -> Result<i64, ArError> {
    let text = core::str::from_utf8(bytes)
        .map_err(|_| ArError::BadNumber)?
        .trim();
    if text.is_empty() {
        return Ok(0);
    }
    text.parse().map_err(|_| ArError::BadNumber)
}

fn parse_octal(bytes: &[u8]) -> Result<u32, ArError> {
    let text = core::str::from_utf8(bytes)
        .map_err(|_| ArError::BadNumber)?
        .trim();
    if text.is_empty() {
        return Ok(0);
    }
    u32::from_str_radix(text, 8).map_err(|_| ArError::BadNumber)
}

/// The symbols a simple ELF32/ELF64 little-endian object exports.
///
/// Non-ELF or unsupported objects yield an empty list; the archive is still valid.
#[must_use]
pub fn elf_symbols(data: &[u8]) -> Vec<String> {
    let mut symbols = Vec::new();
    if data.get(..4) != Some(b"\x7fELF") || data.get(5) != Some(&1) {
        return symbols;
    }
    let is_64 = data.get(4) == Some(&2);
    let (shoff, shentsize, shnum, shstrndx) = if is_64 {
        (
            read_u64(data, 0x28),
            read_u16(data, 0x3a),
            read_u16(data, 0x3c),
            read_u16(data, 0x3e),
        )
    } else {
        (
            read_u32(data, 0x20).map(u64::from),
            read_u16(data, 0x2e),
            read_u16(data, 0x30),
            read_u16(data, 0x32),
        )
    };
    let (Some(shoff), Some(shentsize), Some(shnum), Some(_shstrndx)) =
        (shoff, shentsize, shnum, shstrndx)
    else {
        return symbols;
    };
    let shoff = shoff as usize;
    let shentsize = shentsize as usize;
    for index in 0..usize::from(shnum) {
        let base = shoff + index.saturating_mul(shentsize);
        let Some(kind) = read_u32_at(data, base + 4) else {
            break;
        };
        if kind != 2 {
            continue;
        }
        let Some(offset) = (if is_64 {
            read_u64_at(data, base + 0x18).map(|value| value as usize)
        } else {
            read_u32_at(data, base + 0x10).map(|value| value as usize)
        }) else {
            break;
        };
        let Some(size) = (if is_64 {
            read_u64_at(data, base + 0x20).map(|value| value as usize)
        } else {
            read_u32_at(data, base + 0x14).map(|value| value as usize)
        }) else {
            break;
        };
        let Some(link) = read_u32_at(data, base + if is_64 { 0x28 } else { 0x18 }) else {
            break;
        };
        let Some(entsize) = (if is_64 {
            read_u64_at(data, base + 0x38).map(|value| value as usize)
        } else {
            read_u32_at(data, base + 0x24).map(|value| value as usize)
        }) else {
            break;
        };
        let str_base = shoff
            .checked_add((link as usize).saturating_mul(shentsize))
            .and_then(|base| {
                if is_64 {
                    read_u64_at(data, base + 0x18).map(|value| value as usize)
                } else {
                    read_u32_at(data, base + 0x10).map(|value| value as usize)
                }
            });
        let Some(str_base) = str_base else {
            break;
        };
        let count = if entsize == 0 { 0 } else { size / entsize };
        for symbol in 0..count {
            let entry = offset + symbol.saturating_mul(entsize);
            let (st_name, st_info, st_shndx) = if is_64 {
                (
                    read_u32_at(data, entry),
                    data.get(entry + 4).copied(),
                    read_u16_at(data, entry + 6),
                )
            } else {
                (
                    read_u32_at(data, entry),
                    data.get(entry + 12).copied(),
                    read_u16_at(data, entry + 14),
                )
            };
            let (Some(st_name), Some(st_info), Some(st_shndx)) = (st_name, st_info, st_shndx)
            else {
                continue;
            };
            let binding = st_info >> 4;
            let kind = st_info & 0x0f;
            if binding != 1 && binding != 2 {
                continue;
            }
            if st_shndx == 0 || st_shndx >= 0xff00 || (kind != 0 && kind != 1 && kind != 2) {
                continue;
            }
            let start = str_base.saturating_add(st_name as usize);
            let Some(tail) = data.get(start..) else {
                continue;
            };
            let Some(length) = tail.iter().position(|byte| *byte == 0) else {
                continue;
            };
            let name = String::from_utf8_lossy(tail.get(..length).unwrap_or(&[])).to_string();
            if !name.is_empty() && !symbols.contains(&name) {
                symbols.push(name);
            }
        }
    }
    symbols
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    read_u32_at(data, offset)
}

fn read_u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    read_u64_at(data, offset)
}

fn read_u64_at(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_u16_at(data: &[u8], offset: usize) -> Option<u16> {
    read_u16(data, offset)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
