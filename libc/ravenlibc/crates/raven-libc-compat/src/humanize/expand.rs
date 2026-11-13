// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2019 John Baldwin <jhb@FreeBSD.org>
// Copyright (c) 2025 Dag-Erling Smørgrav <des@FreeBSD.org>
// All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libutil/expand_number.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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

//! `expand_number` (freebsd:lib/libutil/expand_number.c, BSD-2).

use raven_libc_abi::errno::Errno;

use super::errno_of;

/// `expand_number`: parse a scaled number into an `i64`.
///
/// Returns `EINVAL` for a malformed number and `ERANGE` for overflow, exactly like the donor.
#[allow(clippy::manual_ignore_case_cmp)]
pub fn expand_number(s: &[u8]) -> Result<i64, Errno> {
    let mut index = 0;
    while index < s.len() && s[index].is_ascii_whitespace() {
        index = index.wrapping_add(1);
    }
    let mut negative = false;
    if index < s.len() && (s[index] == b'-' || s[index] == b'+') {
        negative = s[index] == b'-';
        index = index.wrapping_add(1);
    }
    if index >= s.len() || !s[index].is_ascii_digit() {
        return Err(errno_of(raven_libc_abi::errno::EINVAL));
    }
    let mut number: u64 = 0;
    // strtoumax(buf, &endptr, 0): an 0x/0X prefix is hexadecimal, a leading 0 is octal.
    let (base, start) = if s.get(index) == Some(&b'0')
        && matches!(s.get(index.wrapping_add(1)), Some(b'x') | Some(b'X'))
    {
        (16u64, index.wrapping_add(2))
    } else if s.get(index) == Some(&b'0') {
        (8u64, index)
    } else {
        (10u64, index)
    };
    index = start;
    let mut digits = 0usize;
    while index < s.len() {
        let digit = match s[index] {
            b'0'..=b'9' => u64::from(s[index].wrapping_sub(b'0')),
            b'a'..=b'f' => u64::from(s[index].wrapping_sub(b'a')).wrapping_add(10),
            b'A'..=b'F' => u64::from(s[index].wrapping_sub(b'A')).wrapping_add(10),
            _ => break,
        };
        if digit >= base {
            break;
        }
        index = index.wrapping_add(1);
        digits = digits.wrapping_add(1);
        number = number
            .checked_mul(base)
            .and_then(|value| value.checked_add(digit))
            .ok_or_else(|| errno_of(raven_libc_abi::errno::ERANGE))?;
    }
    let _ = (start, digits);
    let shift: u32 = if index < s.len() {
        let suffix = s[index];
        match suffix.to_ascii_lowercase() {
            b'e' => 60,
            b'p' => 50,
            b't' => 40,
            b'g' => 30,
            b'm' => 20,
            b'k' => 10,
            _ => 0,
        }
    } else {
        0
    };
    if index < s.len() && b"eptgmkEPTGMK".contains(&s[index]) {
        index = index.wrapping_add(1);
    }
    if index < s.len() && s[index].to_ascii_lowercase() == b'b' {
        index = index.wrapping_add(1);
    }
    if index != s.len() {
        return Err(errno_of(raven_libc_abi::errno::EINVAL));
    }
    let shifted = number
        .checked_shl(shift)
        .filter(|value| value >> shift == number)
        .ok_or_else(|| errno_of(raven_libc_abi::errno::ERANGE))?;
    if negative {
        if shifted > 0x8000_0000_0000_0000 {
            return Err(errno_of(raven_libc_abi::errno::ERANGE));
        }
        Ok((shifted as i64).wrapping_neg())
    } else {
        if shifted > i64::MAX as u64 {
            return Err(errno_of(raven_libc_abi::errno::ERANGE));
        }
        Ok(shifted as i64)
    }
}
