// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2007 David Schultz
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/msun/src/s_nan.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: none)
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
// THIS SOFTWARE IS PROVIDED BY AUTHOR AND CONTRIBUTORS ``AS IS'' AND
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
//! `nan("n-char-seq")`: build a quiet NaN with the payload named by the string.
//!
//! The string is a hex digit sequence with an optional `0x`; digits fill the mantissa from the
//! least significant nibble, high bits overflow away (the `_scan_nan` rule of the donor). An
//! empty or non-hex string gives the default quiet NaN.

/// Build an `f64` NaN from a C `nan()` tag.
pub fn nan(tag: &[u8]) -> f64 {
    let mut digits = tag;
    if digits.len() >= 2 && digits[0] == b'0' && (digits[1] == b'x' || digits[1] == b'X') {
        digits = digits.get(2..).unwrap_or(&[]);
    }
    let mut payload = 0u64;
    for byte in digits {
        let value = match *byte {
            b'0'..=b'9' => u64::from(*byte - b'0'),
            b'a'..=b'f' => u64::from(*byte - b'a') + 10,
            b'A'..=b'F' => u64::from(*byte - b'A') + 10,
            _ => break,
        };
        payload = (payload << 4) | value;
    }
    f64::from_bits(0x7ff8_0000_0000_0000 | (payload & 0x000f_ffff_ffff_ffff))
}

/// Build an `f32` NaN from a C `nanf()` tag.
pub fn nanf(tag: &[u8]) -> f32 {
    let mut digits = tag;
    if digits.len() >= 2 && digits[0] == b'0' && (digits[1] == b'x' || digits[1] == b'X') {
        digits = digits.get(2..).unwrap_or(&[]);
    }
    let mut payload = 0u32;
    for byte in digits {
        let value = match *byte {
            b'0'..=b'9' => u32::from(*byte - b'0'),
            b'a'..=b'f' => u32::from(*byte - b'a') + 10,
            b'A'..=b'F' => u32::from(*byte - b'A') + 10,
            _ => break,
        };
        payload = (payload << 4) | value;
    }
    f32::from_bits(0x7fc0_0000 | (payload & 0x003f_ffff))
}
