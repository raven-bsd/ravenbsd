// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mntent` parsing and formatting (donor-map 15 table T4, 02 X56).
//!
//! CLEANROOM: the escape set and the field split were read from `musl:src/misc/mntent.c`
//! (127 lines, MIT via `musl/COPYRIGHT`; STUDY, not copied). The Linux layout is the ticket's:
//! four whitespace-separated fields, two optional integers, and the `\040`-style octal escapes
//! (a backslash followed by up to three octal digits, `\\` for a backslash, and a lone
//! backslash kept literally). `_PATH_MOUNTED` is `/etc/mtab` (C38's header constant); the shell
//! opens the file.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

/// The default mount table (`_PATH_MOUNTED`).
pub const PATH_MOUNTED: &[u8] = b"/etc/mtab";

/// One `mntent` record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MntEnt {
    /// `mnt_fsname`.
    pub fsname: Vec<u8>,
    /// `mnt_dir`.
    pub dir: Vec<u8>,
    /// `mnt_type`.
    pub type_: Vec<u8>,
    /// `mnt_opts`.
    pub opts: Vec<u8>,
    /// `mnt_freq`.
    pub freq: i32,
    /// `mnt_passno`.
    pub passno: i32,
}

/// `unescape_ent`: decode the octal escapes of one field.
#[must_use]
pub fn unescape(field: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len());
    let mut index = 0usize;
    while index < field.len() {
        if field[index] != b'\\' {
            out.push(field[index]);
            index = index.wrapping_add(1);
            continue;
        }
        if field.get(index.wrapping_add(1)).copied() == Some(b'\\') {
            index = index.wrapping_add(1);
            out.push(field[index]);
            index = index.wrapping_add(1);
            continue;
        }
        let mut value: u8 = 0;
        let mut digits = 0usize;
        let mut cursor = index.wrapping_add(1);
        while digits < 3 {
            match field.get(cursor).copied() {
                Some(c @ b'0'..=b'7') => {
                    value = value.wrapping_mul(8).wrapping_add(c.wrapping_sub(b'0'));
                    cursor = cursor.wrapping_add(1);
                    digits = digits.wrapping_add(1);
                }
                _ => break,
            }
        }
        if digits > 0 && value != 0 {
            out.push(value);
            index = cursor;
        } else {
            out.push(field[index]);
            index = index.wrapping_add(1);
        }
    }
    out
}

fn parse_i32(field: &[u8]) -> i32 {
    let mut index = 0usize;
    let mut negative = false;
    if field.first().copied() == Some(b'-') {
        negative = true;
        index = 1;
    }
    let mut value: i32 = 0;
    while index < field.len() && field[index].is_ascii_digit() {
        value = value
            .wrapping_mul(10)
            .wrapping_add(i32::from(field[index].wrapping_sub(b'0')));
        index = index.wrapping_add(1);
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// `getmntent_r` over one line: `None` for a blank line, a comment or a short line.
#[must_use]
pub fn parse_line(line: &[u8]) -> Option<MntEnt> {
    let mut index = 0usize;
    while index < line.len() && line[index].is_ascii_whitespace() {
        index = index.wrapping_add(1);
    }
    if index >= line.len() || line[index] == b'#' {
        return None;
    }
    let fields: Vec<&[u8]> = line
        .split(|c| c.is_ascii_whitespace())
        .filter(|field| !field.is_empty())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    Some(MntEnt {
        fsname: unescape(fields[0]),
        dir: unescape(fields[1]),
        type_: unescape(fields[2]),
        opts: unescape(fields[3]),
        freq: fields.get(4).map_or(0, |field| parse_i32(field)),
        passno: fields.get(5).map_or(0, |field| parse_i32(field)),
    })
}

/// `hasmntopt`: the offset of `opt` in the comma-separated option list.
#[must_use]
pub fn hasmntopt(opts: &[u8], opt: &[u8]) -> Option<usize> {
    let mut start = 0usize;
    loop {
        let end = opts[start..]
            .iter()
            .position(|&c| c == b',')
            .map_or(opts.len(), |offset| start.wrapping_add(offset));
        let field = opts.get(start..end).unwrap_or(b"");
        if field.starts_with(opt) {
            match field.get(opt.len()).copied() {
                None | Some(b',') | Some(b'=') => return Some(start),
                _ => {}
            }
        }
        if end >= opts.len() {
            return None;
        }
        start = end.wrapping_add(1);
    }
}

/// `addmntent`: `"%s\t%s\t%s\t%s\t%d\t%d\n"`.
pub fn format(entry: &MntEnt, out: &mut Vec<u8>) {
    out.extend_from_slice(&entry.fsname);
    out.push(b'\t');
    out.extend_from_slice(&entry.dir);
    out.push(b'\t');
    out.extend_from_slice(&entry.type_);
    out.push(b'\t');
    out.extend_from_slice(&entry.opts);
    out.push(b'\t');
    push_i32(out, entry.freq);
    out.push(b'\t');
    push_i32(out, entry.passno);
    out.push(b'\n');
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    let mut buf = [0u8; 12];
    let mut index = buf.len();
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    loop {
        index = index.wrapping_sub(1);
        buf[index] = b'0'.wrapping_add(u8::try_from(magnitude % 10).unwrap_or(0));
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if negative {
        index = index.wrapping_sub(1);
        buf[index] = b'-';
    }
    out.extend_from_slice(&buf[index..]);
}

#[cfg(test)]
#[path = "mntent_ut.rs"]
mod tests;
