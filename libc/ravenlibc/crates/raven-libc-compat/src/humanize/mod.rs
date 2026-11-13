// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1997, 1998, 1999, 2002 The NetBSD Foundation, Inc.
// Copyright 2013 John-Mark Gurney <jmg@FreeBSD.org>
// All rights reserved.
//
// RavenBSD: module root of the freebsd:lib/libutil humanize_number/expand_number ports
//
// This code is derived from software contributed to The NetBSD Foundation
// by Jason R. Thorpe of the Numerical Aerospace Simulation Facility,
// NASA Ames Research Center, by Luke Mewburn and by Tomas Svensson.
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

//! `humanize_number` and `expand_number` (donor-map 15 T3, 02 B12).
//!
//! `humanize_number` is the FreeBSD/NetBSD implementation. The only locale use in the donor is
//! `localeconv()->decimal_point`; Raven is UTF-8 only and the decimal point is `.`. The
//! `snprintf` formatting is done by hand (`%d`, `%lld`, `%s`) so the crate does not need a
//! formatter and cannot panic on a malformed input.
//!
//! `expand_number` is the FreeBSD implementation: optional whitespace and sign, `strtoumax`
//! base 0, the suffixes `e p t g m k` (case-insensitive) and an optional trailing `b`.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

/// `HN_DECIMAL` (humanize_number).
pub const HN_DECIMAL: i32 = 0x01;
/// `HN_NOSPACE`.
pub const HN_NOSPACE: i32 = 0x02;
/// `HN_B`.
pub const HN_B: i32 = 0x04;
/// `HN_DIVISOR_1000`.
pub const HN_DIVISOR_1000: i32 = 0x08;
/// `HN_IEC_PREFIXES` (the same bit as [`HN_GETSCALE`]).
pub const HN_IEC_PREFIXES: i32 = 0x10;
/// `HN_GETSCALE`.
pub const HN_GETSCALE: i32 = 0x10;
/// `HN_AUTOSCALE`.
pub const HN_AUTOSCALE: i32 = 0x20;

pub(crate) const MAX_SCALE: i32 = 6;

pub(crate) fn errno_of(value: i32) -> Errno {
    Errno(value as u16)
}

pub(crate) fn push_decimal(out: &mut Vec<u8>, value: i32) {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    let mut magnitude = value.unsigned_abs();
    loop {
        index = index.wrapping_sub(1);
        buf[index] = b'0'.wrapping_add(u8::try_from(magnitude % 10).unwrap_or(0));
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if value < 0 {
        index = index.wrapping_sub(1);
        buf[index] = b'-';
    }
    out.extend_from_slice(&buf[index..]);
}

pub(crate) fn push_i64(out: &mut Vec<u8>, value: i64) {
    let mut buf = [0u8; 24];
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

pub mod expand;
pub mod number;
pub mod scaled;

pub use expand::expand_number;
pub use number::humanize_number;
pub use scaled::{FMT_SCALED_STRSIZE, fmt_scaled, scan_scaled};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
