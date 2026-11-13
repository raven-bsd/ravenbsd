// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1989, 1993 The Regents of the University of California.  All rights reserved.
// Copyright (c) 1999, 2005 The NetBSD Foundation, Inc.  All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/gen/vis.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: vis.c,v 1.88 2024/03/17 21:48:02 andvar Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
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
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `vis`, `svis`, `nvis`, `snvis`, `strvis`, `strsvis`, `strnvis`, `strsnvis` and the
//! `str*x` variants.
//!
//! The donor's wide-character buffer is replaced by a `Vec<u32>` of decoded characters and a
//! `Vec<u8>` of output bytes; `raven-libc-ctype` provides `mbrtowc`, `wcrtomb`, `iswgraph`,
//! `iswcntrl` and the C-locale `isgraph`. All `_DIAGASSERT` calls are dropped (the slices carry
//! the invariants), `__weak_alias` is not translated, and `stravis` is not exported (it is not in
//! the C65 export table). The character sets of the `VIS_GLOB`/`VIS_SHELL` extras are the donor's.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

use raven_libc_ctype::{MbMode, MbState, Mbr, isgraph, iswcntrl, iswgraph, mbrtowc, wcrtomb};

use super::flags::*;

/// The extra characters `VIS_GLOB` forces to be encoded.
const CHAR_GLOB: &[u8] = b"*?[#";
/// The extra characters `VIS_SHELL` forces to be encoded.
const CHAR_SHELL: &[u8] = b"'`\";&<>()|{}]\\$!^~";

/// The maximum number of encoded bytes one input byte can produce (the donor's worst case is
/// `\M-^@`, five bytes; the man page's "4 * len + 1" is an underestimate).
pub const VIS_MAX_EXPANSION: usize = 5;

/// The mode of the character classifier: UTF-8 (`iswgraph`) or the C locale (`isgraph`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Classify {
    Wide,
    Byte,
}

fn is_octal(c: u32) -> bool {
    (b'0' as u32..=b'7' as u32).contains(&c)
}

fn is_white(c: u32) -> bool {
    c == b' ' as u32 || c == b'\t' as u32 || c == b'\n' as u32
}

fn is_safe(c: u32) -> bool {
    c == 0x08 || c == 0x07 || c == b'\r' as u32
}

fn is_graph(mode: Classify, c: u32) -> bool {
    match mode {
        Classify::Byte => c <= 0x7f && isgraph(c as i32),
        Classify::Wide => iswgraph(c),
    }
}

fn is_cntrl(mode: Classify, c: u32) -> bool {
    match mode {
        Classify::Byte => c <= 0x7f && iswcntrl(c),
        Classify::Wide => iswcntrl(c),
    }
}

/// The `extra` list of `makeextralist`: caller characters plus the flag-driven sets.
fn make_extra(flags: u32, caller: &[u8]) -> Vec<u32> {
    let mode = if flags & VIS_NOLOCALE != 0 {
        Classify::Byte
    } else {
        Classify::Wide
    };
    let mut extra = Vec::new();
    let mut state = MbState::default();
    let mut offset = 0;
    while offset < caller.len() {
        match mbrtowc(&mut state, MbMode::Utf8, &caller[offset..]) {
            Mbr::Char { wc, used } => {
                extra.push(wc);
                offset = offset.wrapping_add(used);
            }
            _ => {
                extra.push(u32::from(caller[offset]));
                offset = offset.wrapping_add(1);
                state = MbState::default();
            }
        }
    }
    let _ = mode;
    let mut push_ascii = |bytes: &[u8]| {
        for &b in bytes {
            extra.push(u32::from(b));
        }
    };
    if flags & VIS_GLOB != 0 {
        push_ascii(CHAR_GLOB);
    }
    if flags & VIS_SHELL != 0 {
        push_ascii(CHAR_SHELL);
    }
    if flags & VIS_SP != 0 {
        extra.push(u32::from(b' '));
    }
    if flags & VIS_TAB != 0 {
        extra.push(u32::from(b'\t'));
    }
    if flags & VIS_NL != 0 {
        extra.push(u32::from(b'\n'));
    }
    if flags & VIS_DQ != 0 {
        extra.push(u32::from(b'"'));
    }
    if flags & VIS_NOSLASH == 0 {
        extra.push(u32::from(b'\\'));
    }
    extra
}

fn hex_lower(c: u32) -> u8 {
    b"0123456789abcdef"[c as usize & 0xf]
}

fn hex_upper(c: u32) -> u8 {
    b"0123456789ABCDEF"[c as usize & 0xf]
}

/// `do_mbyte`: one byte of a (possibly wide) character.
fn do_mbyte(out: &mut Vec<u8>, c: u32, flags: u32, nextc: u32, is_extra: bool, mode: Classify) {
    if flags & VIS_CSTYLE != 0 {
        let escaped = match c {
            0x0a => Some(b'n'),
            0x0d => Some(b'r'),
            0x08 => Some(b'b'),
            0x07 => Some(b'a'),
            0x0b => Some(b'v'),
            0x09 => Some(b't'),
            0x0c => Some(b'f'),
            0x20 => Some(b's'),
            0x00 => Some(b'0'),
            _ => None,
        };
        if let Some(letter) = escaped {
            out.push(b'\\');
            out.push(letter);
            if c == 0 && is_octal(nextc) {
                out.push(b'0');
                out.push(b'0');
            }
            return;
        }
        match c {
            // These characters have a special meaning in VIS_CSTYLE.
            0x6e | 0x72 | 0x62 | 0x61 | 0x76 | 0x74 | 0x66 | 0x73 | 0x30 | 0x4d | 0x5e | 0x24 => {}
            _ => {
                if is_graph(mode, c) && !is_octal(c) {
                    out.push(b'\\');
                    out.push(c as u8);
                    return;
                }
            }
        }
    }
    if is_extra || (c & 0o177) == 0x20 || flags & VIS_OCTAL != 0 {
        out.push(b'\\');
        out.push(u8::try_from((c >> 6) & 0o3).unwrap_or(0).wrapping_add(b'0'));
        out.push(u8::try_from((c >> 3) & 0o7).unwrap_or(0).wrapping_add(b'0'));
        out.push(u8::try_from(c & 0o7).unwrap_or(0).wrapping_add(b'0'));
        return;
    }
    if flags & VIS_NOSLASH == 0 {
        out.push(b'\\');
    }
    let mut c = c;
    if c & 0o200 != 0 {
        c &= 0o177;
        out.push(b'M');
    }
    if is_cntrl(mode, c) {
        out.push(b'^');
        if c == 0o177 {
            out.push(b'?');
        } else {
            out.push(u8::try_from(c).unwrap_or(0).wrapping_add(b'@'));
        }
    } else {
        out.push(b'-');
        out.push(u8::try_from(c).unwrap_or(0));
    }
}

/// `do_svis`: one character, including the donor's multi-byte loop over the wide value.
fn do_svis(out: &mut Vec<u8>, c: u32, flags: u32, nextc: u32, extra: &[u32], byte_mode: bool) {
    let mode = if flags & VIS_NOLOCALE != 0 {
        Classify::Byte
    } else {
        Classify::Wide
    };
    // The donor's `wcschr(extra, c)` also matches the list's terminating NUL, so a NUL byte
    // is always "extra" and takes the octal branch (the CSTYLE case returns earlier).
    let is_extra = c == 0 || extra.contains(&c);
    if !is_extra && (is_graph(mode, c) || is_white(c) || (flags & VIS_SAFE != 0 && is_safe(c))) {
        push_char(out, c, byte_mode);
        return;
    }
    // The donor walks the 64-bit word holding the wide character and emits every non-zero
    // byte; zeros only reach do_mbyte for the low byte.
    let mut mask: u64 = 0;
    for shift in (0u32..8).rev() {
        let shift = shift.wrapping_mul(8);
        mask |= 0xff_u64 << shift;
        if (u64::from(c) & mask) != 0 || shift == 0 {
            let byte = (u64::from(c) & (0xff_u64 << shift)) >> shift;
            do_mbyte(
                out,
                u32::try_from(byte).unwrap_or(0),
                flags,
                nextc,
                is_extra,
                mode,
            );
        }
    }
}

fn push_char(out: &mut Vec<u8>, c: u32, byte_mode: bool) {
    if byte_mode {
        // The donor's output loop writes the non-zero bytes of the wide value, not UTF-8.
        for shift in (0u32..4).rev() {
            let byte = ((c >> shift.wrapping_mul(8)) & 0xff) as u8;
            if byte != 0 || shift == 0 {
                out.push(byte);
            }
        }
        return;
    }
    push_utf8(out, c);
}

fn push_utf8(out: &mut Vec<u8>, c: u32) {
    let mut buf = [0u8; 4];
    match wcrtomb(MbMode::Utf8, c, &mut buf) {
        Ok(len) => out.extend_from_slice(&buf[..len]),
        Err(_) => {
            // The donor's wcrtomb failure path writes the low non-zero bytes of the value.
            for shift in (0u32..4).rev() {
                let byte = ((c >> shift.wrapping_mul(8)) & 0xff) as u8;
                if byte != 0 || shift == 0 {
                    out.push(byte);
                }
            }
        }
    }
}

/// `do_hvis`: HTTP 1808/HTTP style.
fn do_hvis(out: &mut Vec<u8>, c: u32, flags: u32, nextc: u32, extra: &[u32], byte_mode: bool) {
    let safe_ascii = c <= 0x7f && b"$-_.+!*'(),".contains(&u8::try_from(c).unwrap_or(0));
    if raven_libc_ctype::iswalnum(c) || safe_ascii {
        do_svis(out, c, flags, nextc, extra, byte_mode);
    } else {
        out.push(b'%');
        out.push(hex_lower(c >> 4));
        out.push(hex_lower(c));
    }
}

/// `do_mvis`: quoted-printable MIME style.
fn do_mvis(out: &mut Vec<u8>, c: u32, flags: u32, nextc: u32, extra: &[u32], byte_mode: bool) {
    let space_before_eol =
        raven_libc_ctype::iswspace(c) && (nextc == u32::from(b'\r') || nextc == u32::from(b'\n'));
    let out_of_range = !raven_libc_ctype::iswspace(c) && (c < 33 || (c > 60 && c < 62) || c > 126);
    let special = c <= 0x7f && b"#$@[\\]^`{|}~".contains(&(c as u8));
    if c != u32::from(b'\n') && (space_before_eol || out_of_range || special) {
        out.push(b'=');
        out.push(hex_upper(c >> 4));
        out.push(hex_upper(c));
    } else {
        do_svis(out, c, flags, nextc, extra, byte_mode);
    }
}

fn encode_char(out: &mut Vec<u8>, c: u32, flags: u32, nextc: u32, extra: &[u32], byte_mode: bool) {
    if flags & VIS_HTTPSTYLE != 0 {
        do_hvis(out, c, flags, nextc, extra, byte_mode);
    } else if flags & VIS_MIMESTYLE != 0 {
        do_mvis(out, c, flags, nextc, extra, byte_mode);
    } else {
        do_svis(out, c, flags, nextc, extra, byte_mode);
    }
}

/// Decode `src` into wide characters, mirroring `istrsenvisx`'s input loop.
fn decode(src: &[u8], flags: u32) -> (Vec<u32>, bool) {
    if flags & VIS_NOLOCALE != 0 {
        return (src.iter().map(|&b| u32::from(b)).collect(), true);
    }
    let mut chars = Vec::new();
    let mut state = MbState::default();
    let mut offset = 0;
    let mut byte_mode = false;
    while offset < src.len() {
        if byte_mode {
            chars.push(u32::from(src[offset]));
            offset = offset.wrapping_add(1);
            continue;
        }
        match mbrtowc(&mut state, MbMode::Utf8, &src[offset..]) {
            Mbr::Char { wc, used } => {
                chars.push(wc);
                offset = offset.wrapping_add(used.max(1));
            }
            _ => {
                // The donor processes the offending byte as a character and switches the rest
                // of the input (and the output) to byte mode.
                chars.push(u32::from(src[offset]));
                offset = offset.wrapping_add(1);
                byte_mode = true;
                state = MbState::default();
            }
        }
    }
    (chars, byte_mode)
}

fn write_output(out: &[u8], byte_mode: bool, dst: &mut [u8]) -> Option<usize> {
    if dst.len() < out.len() {
        return None;
    }
    let _ = byte_mode;
    dst[..out.len()].copy_from_slice(out);
    Some(out.len())
}

/// Encode one byte with a look-ahead byte (the `vis`/`nvis` contract).
///
/// Writes at most [`VIS_MAX_EXPANSION`] bytes and no NUL. `None` when `dst` is too small.
#[must_use]
pub fn vis(dst: &mut [u8], c: u8, flags: u32, nextc: u8) -> Option<usize> {
    let extra = make_extra(flags, b"");
    let mut out = Vec::new();
    // mbrtowc on a single high byte fails, exactly like the donor's one-byte buffer.
    let (chars, byte_mode) = decode(&[c], flags);
    let ch = chars.first().copied().unwrap_or(u32::from(c));
    let next = u32::from(nextc);
    encode_char(&mut out, ch, flags, next, &extra, byte_mode);
    write_output(&out, false, dst)
}

/// `nvis`: `vis` with an explicit destination length (identical here; the length is the slice).
#[must_use]
pub fn nvis(dst: &mut [u8], c: u8, flags: u32, nextc: u8) -> Option<usize> {
    vis(dst, c, flags, nextc)
}

/// `svis`: like [`vis`] with the caller's extra character list.
#[must_use]
pub fn svis(dst: &mut [u8], c: u8, flags: u32, nextc: u8, extra: &[u8]) -> Option<usize> {
    let extra = make_extra(flags, extra);
    let (chars, byte_mode) = decode(&[c], flags);
    let ch = chars.first().copied().unwrap_or(u32::from(c));
    let mut out = Vec::new();
    encode_char(&mut out, ch, flags, u32::from(nextc), &extra, byte_mode);
    write_output(&out, false, dst)
}

/// `snvis`: `svis` with an explicit destination length.
#[must_use]
pub fn snvis(dst: &mut [u8], c: u8, flags: u32, nextc: u8, extra: &[u8]) -> Option<usize> {
    svis(dst, c, flags, nextc, extra)
}

fn encode_string(dst: &mut [u8], src: &[u8], flags: u32, extra: &[u8]) -> Option<usize> {
    let extra = make_extra(flags, extra);
    let (chars, byte_mode) = decode(src, flags);
    let mut out = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let nextc = chars.get(index.wrapping_add(1)).copied().unwrap_or(0);
        encode_char(&mut out, c, flags, nextc, &extra, byte_mode);
        index = index.wrapping_add(1);
    }
    write_output(&out, byte_mode, dst)
}

/// `strvis`: encode `src` into `dst`, returning the number of bytes written (no NUL).
#[must_use]
pub fn strvis(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    encode_string(dst, src, flags, b"")
}

/// `strsvis`: `strvis` with the caller's extra character list.
#[must_use]
pub fn strsvis(dst: &mut [u8], src: &[u8], flags: u32, extra: &[u8]) -> Option<usize> {
    encode_string(dst, src, flags, extra)
}

/// `strvisx`: encode exactly `len` bytes of `src`.
#[must_use]
pub fn strvisx(dst: &mut [u8], src: &[u8], len: usize, flags: u32) -> Option<usize> {
    let end = core::cmp::min(len, src.len());
    encode_string(dst, &src[..end], flags, b"")
}

/// `strnvis`: encode `src`, `None` when `dst` cannot hold the result.
#[must_use]
pub fn strnvis(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    strvis(dst, src, flags)
}

/// `strsnvis`: `strnvis` with an extra character list.
#[must_use]
pub fn strsnvis(dst: &mut [u8], src: &[u8], flags: u32, extra: &[u8]) -> Option<usize> {
    strsvis(dst, src, flags, extra)
}

/// `strnvisx`: `strnvis` over exactly `len` bytes.
#[must_use]
pub fn strnvisx(dst: &mut [u8], src: &[u8], len: usize, flags: u32) -> Option<usize> {
    strvisx(dst, src, len, flags)
}

/// `strsvisx`: `strsvis` over exactly `len` bytes.
#[must_use]
pub fn strsvisx(dst: &mut [u8], src: &[u8], len: usize, flags: u32, extra: &[u8]) -> Option<usize> {
    let end = core::cmp::min(len, src.len());
    strsvis(dst, &src[..end], flags, extra)
}

/// `strsnvisx`: `strnvisx` with an extra character list.
#[must_use]
pub fn strsnvisx(
    dst: &mut [u8],
    src: &[u8],
    len: usize,
    flags: u32,
    extra: &[u8],
) -> Option<usize> {
    strsvisx(dst, src, len, flags, extra)
}

#[cfg(test)]
#[path = "vis_ut.rs"]
mod tests;
