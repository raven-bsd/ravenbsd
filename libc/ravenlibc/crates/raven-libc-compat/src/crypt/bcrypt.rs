// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright 1997 Niels Provos <provos@physnet.uni-hamburg.de>
// All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libcrypt/bcrypt.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: bcrypt.c,v 1.23 2025/12/31 13:02:20 nia Exp $)
//
// The donor accepts only the 'a' minor; the 'b' minor is added per ticket 5.C (password cap 72
// bytes for 'b', NUL-included length for 'a'). The arc4random() call of bcrypt_gensalt is the
// Entropy seam. `bcrypt_checkpass`/`bcrypt_newhash` are OpenBSD-only and are not exported.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. The name of the author may not be used to endorse or promote products
//    derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
// IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
// OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
// IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
// NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
// THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Blowfish-crypt (`$2a$`, `$2b$`), from `netbsd:lib/libcrypt/bcrypt.c`.
//!
//! `BCRYPT_MAXSALT 16`, `BCRYPT_BLOCKS 6`, `BCRYPT_MINROUNDS 16` (log rounds >= 4), the
//! `OrpheanBeholderScryDoubt` plaintext and the donor base64 alphabet are the donor's. Log
//! rounds above 31 or below 4 give `EINVAL` in the dispatch. The key schedule is zeroed before
//! the function returns.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // fixed-size digest/state arrays and the donor round counters
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use crate::seams::Entropy;
use crate::zero::scrub_u32;

use super::blowfish::Blowfish;

/// `BCRYPT_MAXSALT`.
pub const BCRYPT_MAXSALT: usize = 16;
/// `BCRYPT_MAXSALTLEN` (including the NUL).
pub const BCRYPT_MAXSALTLEN: usize = 30;
/// `BCRYPT_BLOCKS`.
pub const BCRYPT_BLOCKS: usize = 6;
/// `BCRYPT_MINROUNDS`: the smallest accepted `2^logr`.
pub const BCRYPT_MINROUNDS: u32 = 16;

const BASE64_CODE: &[u8] = b"./ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

const INDEX_64: [u8; 128] = [
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 0, 1, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 255, 255,
    255, 255, 255, 255, 255, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 25, 26, 27, 255, 255, 255, 255, 255, 255, 28, 29, 30, 31, 32, 33, 34, 35, 36,
    37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 255, 255, 255, 255, 255,
];

#[allow(clippy::indexing_slicing)] // the table is 128 entries and the index is checked
fn char64(c: u8) -> u8 {
    if c > 127 {
        255
    } else {
        INDEX_64[usize::from(c)]
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn decode_base64(buffer: &mut [u8], data: &[u8]) {
    let mut bp = 0usize;
    let mut p = 0usize;
    while bp < buffer.len() {
        let c1 = char64(data.get(p).copied().unwrap_or(0));
        let c2 = char64(data.get(p.wrapping_add(1)).copied().unwrap_or(0));
        if c1 == 255 || c2 == 255 {
            break;
        }
        buffer[bp] = (c1 << 2) | ((c2 & 0x30) >> 4);
        bp = bp.wrapping_add(1);
        if bp >= buffer.len() {
            break;
        }
        let c3 = char64(data.get(p.wrapping_add(2)).copied().unwrap_or(0));
        if c3 == 255 {
            break;
        }
        buffer[bp] = ((c2 & 0x0f) << 4) | ((c3 & 0x3c) >> 2);
        bp = bp.wrapping_add(1);
        if bp >= buffer.len() {
            break;
        }
        let c4 = char64(data.get(p.wrapping_add(3)).copied().unwrap_or(0));
        if c4 == 255 {
            break;
        }
        buffer[bp] = ((c3 & 0x03) << 6) | c4;
        bp = bp.wrapping_add(1);
        p = p.wrapping_add(4);
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn encode_base64(out: &mut Vec<u8>, data: &[u8]) {
    let mut p = 0usize;
    while p < data.len() {
        let c1 = data[p];
        p = p.wrapping_add(1);
        out.push(BASE64_CODE[usize::from(c1 >> 2)]);
        let mut carry = (c1 & 0x03) << 4;
        if p >= data.len() {
            out.push(BASE64_CODE[usize::from(carry)]);
            break;
        }
        let c2 = data[p];
        p = p.wrapping_add(1);
        carry |= (c2 >> 4) & 0x0f;
        out.push(BASE64_CODE[usize::from(carry)]);
        carry = (c2 & 0x0f) << 2;
        if p >= data.len() {
            out.push(BASE64_CODE[usize::from(carry)]);
            break;
        }
        let c3 = data[p];
        p = p.wrapping_add(1);
        carry |= (c3 >> 6) & 0x03;
        out.push(BASE64_CODE[usize::from(carry)]);
        out.push(BASE64_CODE[usize::from(c3 & 0x3f)]);
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn encode_salt(out: &mut Vec<u8>, csalt: &[u8], logr: u8) {
    out.extend_from_slice(b"$2a$");
    out.push(b'0'.wrapping_add(logr / 10));
    out.push(b'0'.wrapping_add(logr % 10));
    out.push(b'$');
    encode_base64(out, csalt);
}

/// `bcrypt_gensalt`: a `$2a$NN$<22 salt chars>` setting.
///
/// `log_rounds` is clamped to 4..=31 like the donor's `__gensalt_blowfish`; the salt bytes come
/// from `entropy` (four per 32-bit draw).
pub fn bcrypt_gensalt<E: Entropy>(
    entropy: &mut E,
    log_rounds: u8,
    out: &mut [u8; BCRYPT_MAXSALTLEN],
) -> Result<usize, Errno> {
    let logr = log_rounds.clamp(4, 31);
    let mut csalt = [0u8; BCRYPT_MAXSALT];
    let mut index = 0usize;
    while index < BCRYPT_MAXSALT {
        let mut seed = [0u8; 4];
        entropy.fill(&mut seed);
        let seed = u32::from_le_bytes(seed);
        for byte in 0..4usize {
            csalt[index] = (seed >> (byte * 8)) as u8;
            index = index.wrapping_add(1);
        }
    }
    let mut text = Vec::new();
    encode_salt(&mut text, &csalt, logr);
    if text.len().wrapping_add(1) > out.len() {
        return Err(Errno(raven_libc_abi::errno::ENOSPC as u16));
    }
    out[..text.len()].copy_from_slice(&text);
    out[text.len()] = 0;
    Ok(text.len())
}

/// `__bcrypt`: the `$2a$`/`$2b$` format.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the donor's fixed-size state
pub fn bcrypt(key: &[u8], salt: &[u8]) -> Option<Vec<u8>> {
    if salt.first().copied() != Some(b'$') {
        return None;
    }
    let mut position = 1usize;
    if salt.get(position).copied()? > b'2' {
        return None;
    }
    let minor;
    if salt.get(position.wrapping_add(1)).copied() != Some(b'$') {
        match salt.get(position.wrapping_add(1)).copied()? {
            b'a' | b'b' => {
                minor = Some(salt[position + 1]);
                position = position.wrapping_add(1);
            }
            _ => return None,
        }
    } else {
        minor = None;
    }
    position = position.wrapping_add(2);
    if salt.get(position.wrapping_add(2)).copied() != Some(b'$') {
        return None;
    }
    let mut logr: u32 = 0;
    let mut digits = 0usize;
    while let Some(digit) = salt.get(position.wrapping_add(digits)) {
        if !digit.is_ascii_digit() {
            break;
        }
        logr = logr
            .wrapping_mul(10)
            .wrapping_add(u32::from(digit.wrapping_sub(b'0')));
        digits = digits.wrapping_add(1);
    }
    if digits != 2 || logr > 31 {
        return None;
    }
    let logr = u8::try_from(logr).unwrap_or(0);
    let rounds = 1u32.checked_shl(u32::from(logr)).unwrap_or(0);
    if rounds < BCRYPT_MINROUNDS {
        return None;
    }
    position = position.wrapping_add(3);
    let salt_text = salt.get(position..)?;
    // strlen(salt) * 3 / 4 < BCRYPT_MAXSALT
    let strength = salt_text.len().wrapping_mul(3).wrapping_div(4);
    if strength < BCRYPT_MAXSALT {
        return None;
    }
    let mut csalt = [0u8; BCRYPT_MAXSALT];
    decode_base64(&mut csalt, salt_text);

    // 'a' includes the NUL in the key length; 'b' caps at 72 bytes without it.
    let mut keybuf = key
        .get(..core::cmp::min(key.len(), 72))
        .unwrap_or(key)
        .to_vec();
    if minor == Some(b'a') {
        keybuf.push(0);
    }
    let mut state = Blowfish::new();
    state.expandstate(&csalt, &keybuf);
    let mut round = 0u32;
    while round < rounds {
        state.expand0state(&keybuf);
        state.expand0state(&csalt);
        round = round.wrapping_add(1);
    }
    let mut ciphertext = *b"OrpheanBeholderScryDoubt";
    let mut cdata = [0u32; BCRYPT_BLOCKS];
    let mut current = 0usize;
    for (index, slot) in cdata.iter_mut().enumerate() {
        let base = index.wrapping_mul(4);
        *slot = Blowfish::stream2word_pub(&ciphertext[base..base.wrapping_add(4)], &mut current);
    }
    let mut encrypted = 0u32;
    while encrypted < 64 {
        state.encrypt(&mut cdata);
        encrypted = encrypted.wrapping_add(1);
    }
    for (index, value) in cdata.iter().enumerate() {
        let base = index.wrapping_mul(4);
        ciphertext[base] = (value >> 24) as u8;
        ciphertext[base.wrapping_add(1)] = (value >> 16) as u8;
        ciphertext[base.wrapping_add(2)] = (value >> 8) as u8;
        ciphertext[base.wrapping_add(3)] = *value as u8;
    }

    let mut out = Vec::new();
    out.push(b'$');
    out.push(b'2');
    if let Some(minor) = minor {
        out.push(minor);
    }
    out.push(b'$');
    out.push(b'0'.wrapping_add(logr / 10));
    out.push(b'0'.wrapping_add(logr % 10));
    out.push(b'$');
    encode_base64(&mut out, &csalt);
    encode_base64(&mut out, &ciphertext[..23]);
    // The key schedule and the plaintext are secret-derived.
    state.zero();
    scrub_u32(&mut cdata);
    crate::zero::scrub(&mut ciphertext);
    Some(out)
}

#[cfg(test)]
#[path = "bcrypt_ut.rs"]
mod tests;
