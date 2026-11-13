// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 1997, 1998, 1999, 2002 The NetBSD Foundation, Inc.
// Copyright 2013 John-Mark Gurney <jmg@FreeBSD.org>
// All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libutil/humanize_number.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $NetBSD: humanize_number.c,v 1.14 2008/04/28 20:22:59 martin Exp $)
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

//! `humanize_number` (freebsd:lib/libutil/humanize_number.c, BSD-2).
//!
//! The only locale use in the donor is `localeconv()->decimal_point`; Raven is UTF-8 only and
//! the decimal point is `.`. `snprintf` is done by hand so the crate needs no formatter.

use alloc::vec::Vec;

use super::{
    HN_AUTOSCALE, HN_B, HN_DECIMAL, HN_DIVISOR_1000, HN_GETSCALE, HN_IEC_PREFIXES, HN_NOSPACE,
    MAX_SCALE, push_decimal, push_i64,
};

/// `humanize_number`.
///
/// Returns the number of characters `snprintf` would have written, or `-1` when the arguments
/// are invalid or `buf` is too short.
pub fn humanize_number(
    buf: &mut [u8],
    quotient: i64,
    suffix: &[u8],
    scale: i32,
    flags: i32,
) -> i32 {
    if scale < 0 {
        return -1;
    }
    if scale > MAX_SCALE && (scale & !(HN_AUTOSCALE | HN_GETSCALE)) != 0 {
        return -1;
    }
    if flags & HN_DIVISOR_1000 != 0 && flags & HN_IEC_PREFIXES != 0 {
        return -1;
    }

    let (baselen_extra, divisor, divisordeccut, prefixes): (usize, i64, i32, &[u8]) =
        if flags & HN_IEC_PREFIXES != 0 {
            let prefixes: &[u8] = if flags & HN_B != 0 {
                b"B\0\0Ki\0Mi\0Gi\0Ti\0Pi\0Ei"
            } else {
                b"\0\0\0Ki\0Mi\0Gi\0Ti\0Pi\0Ei"
            };
            (2, 1024, 973, prefixes)
        } else if flags & HN_DIVISOR_1000 != 0 {
            let prefixes: &[u8] = if flags & HN_B != 0 {
                b"B\0\0k\0\0M\0\0G\0\0T\0\0P\0\0E"
            } else {
                b"\0\0\0k\0\0M\0\0G\0\0T\0\0P\0\0E"
            };
            (1, 1000, 950, prefixes)
        } else {
            let prefixes: &[u8] = if flags & HN_B != 0 {
                b"B\0\0K\0\0M\0\0G\0\0T\0\0P\0\0E"
            } else {
                b"\0\0\0K\0\0M\0\0G\0\0T\0\0P\0\0E"
            };
            (1, 1024, 973, prefixes)
        };

    let mut quotient = quotient;
    let sign: i64;
    let mut baselen = baselen_extra;
    if quotient < 0 {
        sign = -1;
        quotient = quotient.wrapping_neg();
        baselen = baselen.wrapping_add(2);
    } else {
        sign = 1;
        baselen = baselen.wrapping_add(1);
    }
    let sep: &[u8] = if flags & HN_NOSPACE != 0 {
        b""
    } else {
        baselen = baselen.wrapping_add(1);
        b" "
    };
    baselen = baselen.wrapping_add(suffix.len());
    if buf.len() < baselen.wrapping_add(1) {
        return -1;
    }

    let mut remainder: i32 = 0;
    let mut i: i32 = 0;
    if scale & (HN_AUTOSCALE | HN_GETSCALE) != 0 {
        let mut max: i64 = 1;
        let mut columns = buf.len().wrapping_sub(baselen);
        while columns > 0 {
            max = max.wrapping_mul(10);
            columns = columns.wrapping_sub(1);
        }
        while (quotient >= max
            || (quotient == max.wrapping_sub(1)
                && (i64::from(remainder) >= i64::from(divisordeccut)
                    || i64::from(remainder) >= divisor.wrapping_div(2))))
            && i < MAX_SCALE
        {
            remainder = i32::try_from(quotient.wrapping_rem(divisor)).unwrap_or(0);
            quotient = quotient.wrapping_div(divisor);
            i = i.wrapping_add(1);
        }
        if scale & HN_GETSCALE != 0 {
            return i;
        }
    } else {
        while i < scale && i < MAX_SCALE {
            remainder = i32::try_from(quotient.wrapping_rem(divisor)).unwrap_or(0);
            quotient = quotient.wrapping_div(divisor);
            i = i.wrapping_add(1);
        }
    }

    let prefix_at = usize::try_from(i.wrapping_mul(3)).unwrap_or(0);
    let mut prefix_len = 0usize;
    while prefix_at.wrapping_add(prefix_len) < prefixes.len()
        && prefixes.get(prefix_at.wrapping_add(prefix_len)) != Some(&0)
    {
        prefix_len = prefix_len.wrapping_add(1);
    }
    let prefix = prefixes
        .get(prefix_at..prefix_at.wrapping_add(prefix_len))
        .unwrap_or(b"");

    let mut out = Vec::new();
    if ((quotient == 9 && remainder < divisordeccut) || quotient < 9)
        && i > 0
        && flags & HN_DECIMAL != 0
    {
        let half = (divisor as i32).wrapping_div(2);
        let rounded_fraction = remainder.wrapping_mul(10).wrapping_add(half);
        let s1 = i32::try_from(quotient).unwrap_or(0).wrapping_add(
            rounded_fraction
                .wrapping_div(divisor as i32)
                .wrapping_div(10),
        );
        let s2 = rounded_fraction
            .wrapping_div(divisor as i32)
            .wrapping_rem(10);
        push_decimal(&mut out, (sign as i32).wrapping_mul(s1));
        out.push(b'.');
        push_decimal(&mut out, s2);
        out.extend_from_slice(sep);
        out.extend_from_slice(prefix);
        out.extend_from_slice(suffix);
    } else {
        let rounded = quotient.wrapping_add(
            i64::from(remainder)
                .wrapping_add(divisor.wrapping_div(2))
                .wrapping_div(divisor),
        );
        push_i64(&mut out, sign.wrapping_mul(rounded));
        out.extend_from_slice(sep);
        out.extend_from_slice(prefix);
        out.extend_from_slice(suffix);
    }
    let copy = core::cmp::min(out.len(), buf.len());
    buf[..copy].copy_from_slice(&out[..copy]);
    i32::try_from(out.len()).unwrap_or(-1)
}
