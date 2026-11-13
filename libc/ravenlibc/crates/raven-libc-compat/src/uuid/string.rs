// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2002 Marcel Moolenaar
// Copyright (c) 2002 Hiten Mahesh Pandya
// All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/uuid/uuid_from_string.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: uuid_from_string.c,v 1.1 2004/09/13 21:44:54 thorpej Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_from_string` and `uuid_to_string`.

use super::{Status, Uuid, create_nil};

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(c.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(c.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}

fn parse_hex(s: &[u8]) -> Option<u64> {
    let mut value: u64 = 0;
    for &c in s {
        value = value
            .checked_mul(16)
            .and_then(|current| current.checked_add(u64::from(hex_value(c)?)))?;
    }
    Some(value)
}

/// `uuid_from_string`.
///
/// The empty string and the nil string give the nil UUID; anything that is not the exact
/// 36-character form gives [`Status::InvalidString`]; a string whose variant nibble is not
/// 0, `10` or `110` gives [`Status::BadVersion`] (the donor's semantic check).
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the length/hyphen checks bound every index
pub fn from_string(s: &[u8]) -> Result<Uuid, Status> {
    if s.is_empty() {
        return Ok(create_nil());
    }
    if s.len() != 36 {
        return Err(Status::InvalidString);
    }
    if s[8] != b'-' || s[13] != b'-' || s[18] != b'-' || s[23] != b'-' {
        return Err(Status::InvalidString);
    }
    let time_low = parse_hex(&s[0..8]).ok_or(Status::InvalidString)?;
    let time_mid = parse_hex(&s[9..13]).ok_or(Status::InvalidString)?;
    let time_hi = parse_hex(&s[14..18]).ok_or(Status::InvalidString)?;
    let seq_hi = parse_hex(&s[19..21]).ok_or(Status::InvalidString)?;
    let seq_low = parse_hex(&s[21..23]).ok_or(Status::InvalidString)?;
    let mut node = [0u8; 6];
    for (index, byte) in node.iter_mut().enumerate() {
        let start = 24usize.wrapping_add(index.wrapping_mul(2));
        *byte = parse_hex(&s[start..start.wrapping_add(2)]).ok_or(Status::InvalidString)? as u8;
    }
    let uuid = Uuid {
        time_low: u32::try_from(time_low).map_err(|_| Status::InvalidString)?,
        time_mid: u16::try_from(time_mid).map_err(|_| Status::InvalidString)?,
        time_hi_and_version: u16::try_from(time_hi).map_err(|_| Status::InvalidString)?,
        clock_seq_hi_and_reserved: seq_hi as u8,
        clock_seq_low: seq_low as u8,
        node,
    };
    let variant = uuid.clock_seq_hi_and_reserved;
    if (variant & 0x80) != 0x00 && (variant & 0xc0) != 0x80 && (variant & 0xe0) != 0xc0 {
        return Err(Status::BadVersion);
    }
    Ok(uuid)
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the caller's offsets are within 37
fn push_hex(out: &mut [u8], start: usize, value: u64, digits: usize) {
    const HEX: &[u8] = b"0123456789abcdef";
    for index in 0..digits {
        let shift = (digits.wrapping_sub(1).wrapping_sub(index)).wrapping_mul(4);
        out[start.wrapping_add(index)] = HEX[((value >> shift) & 0xf) as usize];
    }
}

/// `uuid_to_string`: lower-case, exactly 36 characters plus the NUL at index 36.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the output is a fixed 37-byte buffer
pub fn to_string(u: &Uuid, out: &mut [u8; 37]) {
    push_hex(out, 0, u64::from(u.time_low), 8);
    out[8] = b'-';
    push_hex(out, 9, u64::from(u.time_mid), 4);
    out[13] = b'-';
    push_hex(out, 14, u64::from(u.time_hi_and_version), 4);
    out[18] = b'-';
    push_hex(out, 19, u64::from(u.clock_seq_hi_and_reserved), 2);
    push_hex(out, 21, u64::from(u.clock_seq_low), 2);
    out[23] = b'-';
    for (index, &byte) in u.node.iter().enumerate() {
        push_hex(
            out,
            24usize.wrapping_add(index.wrapping_mul(2)),
            u64::from(byte),
            2,
        );
    }
    out[36] = 0;
}

#[cfg(test)]
#[path = "string_ut.rs"]
mod tests;
