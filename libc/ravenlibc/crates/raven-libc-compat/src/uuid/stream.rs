// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2002 Marcel Moolenaar
// Copyright (c) 2002 Hiten Mahesh Pandya
// All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/uuid/uuid_stream.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: uuid_stream.c,v 1.3 2008/04/19 18:21:38 plunky Exp $)
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

//! `uuid_enc_le`, `uuid_dec_le`, `uuid_enc_be`, `uuid_dec_be`.
//!
//! The sequence field is big-endian in the octet stream; the time fields follow the donor's
//! `le32enc`/`be32enc` choice.

use super::Uuid;

/// `uuid_enc_le`.
pub fn enc_le(u: &Uuid, buf: &mut [u8; 16]) {
    buf[0..4].copy_from_slice(&u.time_low.to_le_bytes());
    buf[4..6].copy_from_slice(&u.time_mid.to_le_bytes());
    buf[6..8].copy_from_slice(&u.time_hi_and_version.to_le_bytes());
    buf[8] = u.clock_seq_hi_and_reserved;
    buf[9] = u.clock_seq_low;
    buf[10..16].copy_from_slice(&u.node);
}

/// `uuid_dec_le`.
#[must_use]
pub fn dec_le(buf: &[u8; 16]) -> Uuid {
    Uuid {
        time_low: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        time_mid: u16::from_le_bytes([buf[4], buf[5]]),
        time_hi_and_version: u16::from_le_bytes([buf[6], buf[7]]),
        clock_seq_hi_and_reserved: buf[8],
        clock_seq_low: buf[9],
        node: [buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]],
    }
}

/// `uuid_enc_be`.
pub fn enc_be(u: &Uuid, buf: &mut [u8; 16]) {
    buf[0..4].copy_from_slice(&u.time_low.to_be_bytes());
    buf[4..6].copy_from_slice(&u.time_mid.to_be_bytes());
    buf[6..8].copy_from_slice(&u.time_hi_and_version.to_be_bytes());
    buf[8] = u.clock_seq_hi_and_reserved;
    buf[9] = u.clock_seq_low;
    buf[10..16].copy_from_slice(&u.node);
}

/// `uuid_dec_be`.
#[must_use]
pub fn dec_be(buf: &[u8; 16]) -> Uuid {
    Uuid {
        time_low: u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
        time_mid: u16::from_be_bytes([buf[4], buf[5]]),
        time_hi_and_version: u16::from_be_bytes([buf[6], buf[7]]),
        clock_seq_hi_and_reserved: buf[8],
        clock_seq_low: buf[9],
        node: [buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]],
    }
}

#[cfg(test)]
#[path = "stream_ut.rs"]
mod tests;
