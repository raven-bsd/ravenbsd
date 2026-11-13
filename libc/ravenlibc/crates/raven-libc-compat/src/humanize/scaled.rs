// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2001, 2002, 2003 Ian F. Darwin.  All rights reserved.
//
// RavenBSD: transpiled from netbsd:crypto/external/bsd/openssh/dist/fmt_scaled.c
// @ 36eddebcc36cb0775aa2962005b9c05419a6ff01 (the OpenBSD original is
// openbsd:lib/libutil/fmt_scaled.c; the FreeBSD/OpenBSD libutil checkout is absent)
// (file revision tags: $NetBSD: fmt_scaled.c,v 1.8 2019/01/27 02:08:33 pgoyette Exp $,
// $OpenBSD: fmt_scaled.c,v 1.9 2007/03/20 03:42:52 tedu Exp $)
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

//! `fmt_scaled`/`scan_scaled` (OpenSSH's copy of openbsd:lib/libutil/fmt_scaled.c, BSD-3).

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use super::{errno_of, push_i64};

/// `FMT_SCALED_STRSIZE`.
pub const FMT_SCALED_STRSIZE: usize = 40;

/// `fmt_scaled`: format `n` as `<number><B|K|M|G|T|P|E>`.
pub fn fmt_scaled(n: i64, buf: &mut [u8]) -> Result<(), Errno> {
    const SCALE_CHARS: &[u8] = b"BKMGTPE";
    const SCALE_FACTORS: [i64; 7] = [
        1,
        1024,
        1_048_576,
        1_073_741_824,
        1_099_511_627_776,
        1_125_899_906_842_624,
        1_152_921_504_606_846_976,
    ];
    let mut number = n;
    let abval = if number < 0 {
        number.wrapping_neg()
    } else {
        number
    };
    if abval < 0 || abval.wrapping_div(1024) >= SCALE_FACTORS[6] {
        return Err(errno_of(raven_libc_abi::errno::ERANGE));
    }
    let mut unit = 0usize;
    let mut fract: i64 = 0;
    for (index, factor) in SCALE_FACTORS.iter().enumerate() {
        if abval.wrapping_div(1024) < *factor {
            unit = index;
            fract = if index == 0 {
                0
            } else {
                abval.wrapping_rem(*factor)
            };
            number = number.wrapping_div(*factor);
            if index > 0 {
                fract = fract.wrapping_div(SCALE_FACTORS[index.wrapping_sub(1)]);
            }
            break;
        }
    }
    fract = fract.wrapping_mul(10).wrapping_add(512).wrapping_div(1024);
    if fract == 10 {
        number = if number >= 0 {
            number.wrapping_add(1)
        } else {
            number.wrapping_sub(1)
        };
        fract = 0;
    }
    let mut out = Vec::new();
    if number == 0 {
        out.extend_from_slice(b"0B");
    } else if unit == 0 || number >= 100 || number <= -100 {
        if fract >= 5 {
            number = if number >= 0 {
                number.wrapping_add(1)
            } else {
                number.wrapping_sub(1)
            };
        }
        push_i64(&mut out, number);
        out.push(*SCALE_CHARS.get(unit).unwrap_or(&b'B'));
    } else {
        push_i64(&mut out, number);
        out.push(b'.');
        push_i64(&mut out, fract);
        out.push(*SCALE_CHARS.get(unit).unwrap_or(&b'B'));
    }
    if buf.len() < out.len().wrapping_add(1) {
        return Err(errno_of(raven_libc_abi::errno::ERANGE));
    }
    buf[..out.len()].copy_from_slice(&out);
    buf[out.len()] = 0;
    Ok(())
}

/// `scan_scaled`: parse `<number>[.<fraction>][B|K|M|G|T|P|E]`.
pub fn scan_scaled(s: &[u8]) -> Result<i64, Errno> {
    const SCALE_CHARS: &[u8] = b"BKMGTPE";
    const SCALE_FACTORS: [i64; 7] = [
        1,
        1024,
        1_048_576,
        1_073_741_824,
        1_099_511_627_776,
        1_125_899_906_842_624,
        1_152_921_504_606_846_976,
    ];
    const MAX_DIGITS: u32 = 21;
    let mut index = 0;
    while index < s.len() && s[index].is_ascii_whitespace() {
        index = index.wrapping_add(1);
    }
    let mut sign: i64 = 0;
    while index < s.len() && (s[index] == b'-' || s[index] == b'+') {
        if sign != 0 {
            return Err(errno_of(raven_libc_abi::errno::EINVAL));
        }
        sign = if s[index] == b'-' { -1 } else { 1 };
        index = index.wrapping_add(1);
    }
    let mut ndigits: u32 = 0;
    let mut fract_digits: u32 = 0;
    let mut whole: i64 = 0;
    let mut fpart: i64 = 0;
    while index < s.len() && (s[index].is_ascii_digit() || s[index] == b'.') {
        if s[index] == b'.' {
            if fract_digits > 0 {
                return Err(errno_of(raven_libc_abi::errno::EINVAL));
            }
            fract_digits = 1;
            index = index.wrapping_add(1);
            continue;
        }
        let digit = i64::from(s[index].wrapping_sub(b'0'));
        if fract_digits > 0 {
            if fract_digits >= MAX_DIGITS.wrapping_sub(1) {
                index = index.wrapping_add(1);
                continue;
            }
            fract_digits = fract_digits.wrapping_add(1);
            fpart = fpart.wrapping_mul(10).wrapping_add(digit);
        } else {
            ndigits = ndigits.wrapping_add(1);
            if ndigits >= MAX_DIGITS {
                return Err(errno_of(raven_libc_abi::errno::ERANGE));
            }
            whole = whole.wrapping_mul(10).wrapping_add(digit);
        }
        index = index.wrapping_add(1);
    }
    if sign != 0 {
        whole = whole.wrapping_mul(sign);
        fpart = fpart.wrapping_mul(sign);
    }
    if index >= s.len() {
        return Ok(whole);
    }
    for (unit, factor) in SCALE_FACTORS.iter().enumerate() {
        let scale = *SCALE_CHARS.get(unit).unwrap_or(&b'B');
        if s[index] == scale || s[index] == scale.to_ascii_lowercase() {
            if s.get(index.wrapping_add(1))
                .is_some_and(u8::is_ascii_alphanumeric)
            {
                return Err(errno_of(raven_libc_abi::errno::EINVAL));
            }
            let mut scaled_whole = whole.wrapping_mul(*factor);
            let mut scaled_fract = fpart;
            while scaled_fract >= i64::MAX.wrapping_div(*factor) {
                scaled_fract = scaled_fract.wrapping_div(10);
                fract_digits = fract_digits.saturating_sub(1);
            }
            scaled_fract = scaled_fract.wrapping_mul(*factor);
            if fract_digits > 0 {
                let mut count = fract_digits.wrapping_sub(1);
                while count > 0 {
                    scaled_fract = scaled_fract.wrapping_div(10);
                    count = count.wrapping_sub(1);
                }
            }
            scaled_whole = scaled_whole.wrapping_add(scaled_fract);
            return Ok(scaled_whole);
        }
    }
    Err(errno_of(raven_libc_abi::errno::ERANGE))
}
