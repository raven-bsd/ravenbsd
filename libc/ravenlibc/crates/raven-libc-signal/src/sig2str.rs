// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2025 Ricardo Branco <rbranco@suse.de>.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/sig2str.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//      notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//      notice, this list of conditions and the following disclaimer in the
//      documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! `sig2str` and `str2sig`, Linux numbering.
//!
//! The FreeBSD donor indexes `sys_signame[]` with BSD numbers and formats the real-time range as
//! `RTMIN`, `RTMIN+n`, `RTMAX-n`, `RTMAX` around the midpoint `(SIGRTMIN+SIGRTMAX)/2`. Here the
//! table is the Linux table of [`crate::names`], `SIGRTMIN` is 35, and 32..34 are refused
//! (A-sig-2). `str2sig` also accepts decimal numbers (POSIX.1-2024). `SIG2STR_MAX` is 32.

use crate::errors::EINVAL_ERR;
use crate::names::{name_of, number_of, strip_sig_prefix};
use crate::sigset::{SIGRTMAX, SIGRTMIN};
use raven_libc_abi::errno::Errno;

/// `SIG2STR_MAX` (`freebsd:include/signal.h`): the buffer size the C caller passes.
pub const SIG2STR_MAX: usize = 32;

const RTMIN_STR: &[u8] = b"RTMIN";
const RTMAX_STR: &[u8] = b"RTMAX";

/// Write a NUL-terminated name for `sig`; the return value is the length without the NUL.
pub fn sig2str(sig: i32, out: &mut [u8; SIG2STR_MAX]) -> Result<usize, Errno> {
    if sig <= 0 || sig > SIGRTMAX {
        return Err(EINVAL_ERR);
    }
    if (32..=SIGRTMIN.wrapping_sub(1)).contains(&sig) {
        // 32..34 are libc-internal and never nameable.
        return Err(EINVAL_ERR);
    }
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = if let Some(name) = name_of(sig) {
        copy_bytes(&mut buffer, name.as_bytes())
    } else if sig < SIGRTMIN {
        write_number_at(&mut buffer, 0, sig)
    } else if sig == SIGRTMIN {
        copy_bytes(&mut buffer, RTMIN_STR)
    } else if sig == SIGRTMAX {
        copy_bytes(&mut buffer, RTMAX_STR)
    } else if sig <= SIGRTMIN.wrapping_add(SIGRTMAX) / 2 {
        let cursor = copy_bytes(&mut buffer, RTMIN_STR);
        let cursor = cursor.saturating_add(write_byte(&mut buffer, cursor, b'+'));
        cursor.saturating_add(write_number_at(
            &mut buffer,
            cursor,
            sig.wrapping_sub(SIGRTMIN),
        ))
    } else {
        let cursor = copy_bytes(&mut buffer, RTMAX_STR);
        let cursor = cursor.saturating_add(write_byte(&mut buffer, cursor, b'-'));
        cursor.saturating_add(write_number_at(
            &mut buffer,
            cursor,
            SIGRTMAX.wrapping_sub(sig),
        ))
    };
    out.fill(0);
    for (slot, byte) in out.iter_mut().zip(buffer.iter()) {
        *slot = *byte;
    }
    if let Some(nul) = out.get_mut(length) {
        *nul = 0;
    }
    Ok(length)
}

/// Parse a signal name, `RTMIN`/`RTMAX` form or decimal number (case-insensitive, optional `SIG`).
pub fn str2sig(text: &[u8]) -> Result<i32, Errno> {
    let text = core::str::from_utf8(text).map_err(|_| EINVAL_ERR)?;
    let stripped = strip_sig_prefix(text.trim());
    if stripped.is_empty() {
        return Err(EINVAL_ERR);
    }
    if let Some(number) = parse_rt(stripped) {
        return number;
    }
    let first = stripped.as_bytes().first().copied();
    if first.is_some_and(|byte| byte.is_ascii_digit()) {
        let value = parse_decimal(stripped).ok_or(EINVAL_ERR)?;
        if (1..=i64::from(SIGRTMAX)).contains(&value) {
            return Ok(value as i32);
        }
        return Err(EINVAL_ERR);
    }
    number_of(stripped).ok_or(EINVAL_ERR)
}

/// `RTMIN`, `RTMAX`, `RTMIN+n`, `RTMAX-n`.
fn parse_rt(text: &str) -> Option<Result<i32, Errno>> {
    let bytes = text.as_bytes();
    let is_min = starts_with_ignore_case(bytes, RTMIN_STR);
    let is_max = starts_with_ignore_case(bytes, RTMAX_STR);
    if !is_min && !is_max {
        return None;
    }
    let base = if is_min { SIGRTMIN } else { SIGRTMAX };
    let rest = match text.get(5..) {
        Some(rest) => rest,
        None => return Some(Err(EINVAL_ERR)),
    };
    if rest.is_empty() {
        return Some(Ok(base));
    }
    let sign = rest.as_bytes().first().copied();
    if sign != Some(b'+') && sign != Some(b'-') {
        return Some(Err(EINVAL_ERR));
    }
    let digits = rest.get(1..).unwrap_or("");
    let value = match parse_decimal(digits) {
        Some(value) if value != 0 => value,
        _ => return Some(Err(EINVAL_ERR)),
    };
    let delta = if sign == Some(b'-') {
        value.checked_neg()?
    } else {
        value
    };
    let signal = i64::from(base).checked_add(delta)?;
    if (i64::from(SIGRTMIN)..=i64::from(SIGRTMAX)).contains(&signal) {
        Some(Ok(signal as i32))
    } else {
        Some(Err(EINVAL_ERR))
    }
}

fn parse_decimal(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for byte in text.as_bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(i64::from(byte.wrapping_sub(b'0')))?;
        if value > i64::from(i32::MAX) {
            return None;
        }
    }
    Some(value)
}

fn starts_with_ignore_case(bytes: &[u8], prefix: &[u8]) -> bool {
    if bytes.len() < prefix.len() {
        return false;
    }
    bytes
        .iter()
        .zip(prefix.iter())
        .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

fn copy_bytes(buffer: &mut [u8; SIG2STR_MAX], bytes: &[u8]) -> usize {
    let mut written = 0usize;
    for (slot, byte) in buffer.iter_mut().zip(bytes.iter()) {
        *slot = *byte;
        written = written.saturating_add(1);
    }
    written
}

fn write_byte(buffer: &mut [u8; SIG2STR_MAX], at: usize, byte: u8) -> usize {
    if let Some(slot) = buffer.get_mut(at) {
        *slot = byte;
        1
    } else {
        0
    }
}

/// Decimal digits with no leading zero (the donor's `snprintf("%d")`).
fn write_number_at(buffer: &mut [u8; SIG2STR_MAX], at: usize, value: i32) -> usize {
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    let mut rest = value.unsigned_abs();
    if rest == 0 {
        digits[0] = b'0';
        count = 1;
    } else {
        while rest > 0 && count < digits.len() {
            if let Some(slot) = digits.get_mut(count) {
                *slot = b'0'.wrapping_add((rest % 10) as u8);
            }
            rest /= 10;
            count = count.saturating_add(1);
        }
    }
    let mut written = 0usize;
    for index in (0..count).rev() {
        if let Some(byte) = digits.get(index) {
            written = written.saturating_add(write_byte(buffer, at.saturating_add(written), *byte));
        }
    }
    written
}

#[cfg(test)]
#[path = "sig2str_ut.rs"]
mod sig2str_ut;
