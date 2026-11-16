// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `uuencode` begin/end encoding.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Maximum input bytes per encoded line.
pub const MAX_LINE: usize = 45;

/// Why an encoded stream or argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Missing `begin` line.
    MissingBegin,
    /// Missing `end` line.
    MissingEnd,
    /// The mode field is not octal.
    BadMode,
    /// The file name is empty.
    BadName,
    /// A line has an invalid length character.
    BadLength,
    /// A line has an invalid encoded character or is truncated.
    BadLine,
    /// Too many or too few operands.
    Usage,
}

/// Parsed `uuencode` operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options<'a> {
    /// Input file; `None` means standard input.
    pub input: Option<&'a str>,
    /// Name written into the `begin` line.
    pub name: &'a str,
}

/// Parse `uuencode [input] output_name`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, Error> {
    let args: Vec<&str> = args.into_iter().collect();
    match args.as_slice() {
        [name] => Ok(Options { input: None, name }),
        [input, name] => Ok(Options {
            input: Some(input),
            name,
        }),
        _ => Err(Error::Usage),
    }
}

/// Encode `data` as one POSIX `uuencode` stream.
#[must_use]
pub fn encode(data: &[u8], name: &str, mode: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("begin {:o} {}\n", mode & 0o7777, name).as_bytes());
    for chunk in data.chunks(MAX_LINE) {
        out.push((chunk.len() as u8).wrapping_add(32));
        for group in chunk.chunks(3) {
            let zero = 0u8;
            let b0 = group.first().copied().unwrap_or(zero);
            let b1 = group.get(1).copied().unwrap_or(zero);
            let b2 = group.get(2).copied().unwrap_or(zero);
            out.push(((b0 >> 2) & 0x3f).wrapping_add(32));
            out.push((((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0f)).wrapping_add(32));
            out.push((((b1 & 0x0f) << 2) | ((b2 >> 6) & 0x03)).wrapping_add(32));
            out.push((b2 & 0x3f).wrapping_add(32));
        }
        out.push(b'\n');
    }
    out.extend_from_slice(b"`\nend\n");
    out
}

/// One decoded `uuencode` stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// Mode from the `begin` line.
    pub mode: u32,
    /// Name from the `begin` line.
    pub name: String,
    /// Decoded bytes.
    pub data: Vec<u8>,
}

/// Decode a POSIX `uuencode` stream.
///
/// # Errors
///
/// Returns [`Error`] for a missing/malformed `begin`, a bad length or character,
/// or a missing `end` line.
pub fn decode(input: &[u8]) -> Result<Decoded, Error> {
    let mut lines = input.split(|byte| *byte == b'\n');
    let mut header = None;
    for line in &mut lines {
        let line = trim_cr(line);
        if line.is_empty() {
            continue;
        }
        header = Some(line);
        break;
    }
    let header = header.ok_or(Error::MissingBegin)?;
    let rest = header.strip_prefix(b"begin ").ok_or(Error::MissingBegin)?;
    let mut fields = rest.splitn(2, |byte| *byte == b' ');
    let mode = fields.next().ok_or(Error::BadMode)?;
    let name = fields.next().ok_or(Error::BadName)?;
    if name.is_empty() {
        return Err(Error::BadName);
    }
    let mode = parse_octal(mode).ok_or(Error::BadMode)?;
    let mut data = Vec::new();
    let mut ended = false;
    for line in lines {
        let line = trim_cr(line);
        if line == b"end" {
            ended = true;
            break;
        }
        if line.is_empty() {
            continue;
        }
        if line[0] == b'`' {
            continue;
        }
        let length = usize::from(line[0].wrapping_sub(32));
        if length == 0 {
            continue;
        }
        if length > MAX_LINE {
            return Err(Error::BadLength);
        }
        let groups = length.div_ceil(3);
        if line.len() < 1 + groups * 4 {
            return Err(Error::BadLine);
        }
        let mut chunk = Vec::with_capacity(groups * 3);
        for group in line[1..].chunks(4).take(groups) {
            if group.len() != 4 {
                return Err(Error::BadLine);
            }
            let mut values = [0u8; 4];
            for (index, byte) in group.iter().enumerate() {
                if !(32..=95).contains(byte) {
                    return Err(Error::BadLine);
                }
                values[index] = byte.wrapping_sub(32) & 0x3f;
            }
            chunk.push((values[0] << 2) | (values[1] >> 4));
            chunk.push(((values[1] & 0x0f) << 4) | (values[2] >> 2));
            chunk.push(((values[2] & 0x03) << 6) | values[3]);
        }
        chunk.truncate(length);
        data.extend_from_slice(&chunk);
    }
    if !ended {
        return Err(Error::MissingEnd);
    }
    Ok(Decoded {
        mode,
        name: core::str::from_utf8(name)
            .map_err(|_| Error::BadName)?
            .to_string(),
        data,
    })
}

fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn parse_octal(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    if bytes.is_empty() {
        return None;
    }
    for byte in bytes {
        if !(b'0'..=b'7').contains(byte) {
            return None;
        }
        value = value.checked_mul(8)?.checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
