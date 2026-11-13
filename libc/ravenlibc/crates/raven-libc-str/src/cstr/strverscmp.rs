// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2022 Aymeric Wibo <obiwac@gmail.com>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/string/strverscmp.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (no $FreeBSD$ tag; SPDX-License-Identifier: BSD-2-Clause in the header)
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

//! `strverscmp`: version-string comparison with the FreeBSD donor's leading-zero rules.

use super::byte_at;

#[inline]
fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

/// Compare two version strings the way FreeBSD's `strverscmp` does.
///
/// The result is negative, zero or positive. On a disagreement with glibc the FreeBSD behaviour
/// is kept (RD-11).
#[must_use]
pub fn strverscmp(a: &[u8], b: &[u8]) -> i32 {
    // C: if (s1 == s2) return (0);
    if core::ptr::eq(a.as_ptr(), b.as_ptr()) && a.len() == b.len() {
        return 0;
    }

    let mut u1 = 0usize;
    let mut u2 = 0usize;

    // C: while (*u1 != '\0' && *u2 != '\0')
    while byte_at(a, u1) != 0 && byte_at(b, u2) != 0 {
        let c1 = byte_at(a, u1);
        let c2 = byte_at(b, u2);

        if !is_digit(c1) || !is_digit(c2) {
            if c1 != c2 {
                return i32::from(c1).wrapping_sub(i32::from(c2));
            }
            u1 = u1.wrapping_add(1);
            u2 = u2.wrapping_add(1);
            continue;
        }

        if c1 == b'0' || c2 == b'0' {
            // Leading zeros are the fractional part: more zeros means a smaller number.
            let mut zeros1 = 0usize;
            let mut zeros2 = 0usize;
            while byte_at(a, u1) == b'0' {
                zeros1 = zeros1.wrapping_add(1);
                u1 = u1.wrapping_add(1);
            }
            while byte_at(b, u2) == b'0' {
                zeros2 = zeros2.wrapping_add(1);
                u2 = u2.wrapping_add(1);
            }
            if zeros1 != zeros2 {
                return (zeros2 as i32).wrapping_sub(zeros1 as i32);
            }
            // C: handle the case where 0 < 09.
            if !is_digit(byte_at(a, u1)) && is_digit(byte_at(b, u2)) {
                return 1;
            }
            if !is_digit(byte_at(b, u2)) && is_digit(byte_at(a, u1)) {
                return -1;
            }
        } else {
            // No leading zeros: count the digits first so 7 < 60.
            let num1 = u1;
            let num2 = u2;
            while is_digit(byte_at(a, u1)) {
                u1 = u1.wrapping_add(1);
            }
            while is_digit(byte_at(b, u2)) {
                u2 = u2.wrapping_add(1);
            }
            let count1 = u1.wrapping_sub(num1);
            let count2 = u2.wrapping_sub(num2);
            if count1 != count2 {
                return (count1 as i32).wrapping_sub(count2 as i32);
            }
            u1 = num1;
            u2 = num2;
        }

        // Compare each digit until there are none left.
        while is_digit(byte_at(a, u1)) && is_digit(byte_at(b, u2)) {
            let d1 = byte_at(a, u1);
            let d2 = byte_at(b, u2);
            if d1 != d2 {
                return i32::from(d1).wrapping_sub(i32::from(d2));
            }
            u1 = u1.wrapping_add(1);
            u2 = u2.wrapping_add(1);
        }
    }
    i32::from(byte_at(a, u1)).wrapping_sub(i32::from(byte_at(b, u2)))
}

#[cfg(test)]
#[path = "strverscmp_ut.rs"]
mod tests;
