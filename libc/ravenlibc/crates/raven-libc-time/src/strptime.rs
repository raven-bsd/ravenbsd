// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 1997, 1998, 2005, 2008 The NetBSD Foundation, Inc.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/time/strptime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01.
//
// This code was contributed to The NetBSD Foundation by Klaus Klein.
// Heavily optimised by David Laight
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
// PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

//! `strptime`: parse a broken-down time from a byte string.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the donor's parser arithmetic is covered by the vectors.
//!
//! Fields the format does not name keep their previous value. `%z`/`%Z` set
//! `tm_gmtoff`/`tm_isdst`; `%P` is the RavenBSD alias of `%p` (RD-28).

use crate::locale::TimeLocale;
use crate::tz::consts::{HOURSPERDAY, SECSPERHOUR, SECSPERMIN, TM_YEAR_BASE};
use crate::tz::convert::localtime_r;
use crate::tz::overflow::isleap_sum;
use crate::tz::{NO_ZONE, Tm, TzState};

/// Context for `%Z` and `%s`.
#[derive(Clone, Copy, Debug)]
pub struct StrptimeCtx<'a> {
    /// The zone's two abbreviations.
    pub tzname: [&'a [u8]; 2],
    /// The `localtime_r` callback used by `%s`.
    pub localtime: Option<crate::sys::LocaltimeFn>,
}

impl Default for StrptimeCtx<'_> {
    fn default() -> Self {
        Self {
            tzname: [b"", b""],
            localtime: None,
        }
    }
}

const S_YEAR: u32 = 1 << 0;
const S_MON: u32 = 1 << 1;
const S_YDAY: u32 = 1 << 2;
const S_MDAY: u32 = 1 << 3;
const S_WDAY: u32 = 1 << 4;
const S_HOUR: u32 = 1 << 5;

const START_OF_MONTH: [[i32; 13]; 2] = [
    [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365],
    [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366],
];

const NAST: [&str; 4] = ["EST", "CST", "MST", "PST"];
const NADT: [&str; 4] = ["EDT", "CDT", "MDT", "PDT"];

fn is_space(byte: u8) -> bool {
    byte.is_ascii_whitespace()
}

fn delim(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| byte == 0 || is_space(byte))
}

fn conv_num(buf: &[u8], dest: &mut i32, low: u32, high: u32) -> Option<usize> {
    let mut result = 0u32;
    let mut rule = high;
    let mut index = 0usize;
    let mut byte = *buf.get(index)?;
    if !byte.is_ascii_digit() {
        return None;
    }
    loop {
        result = result.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'));
        rule /= 10;
        index += 1;
        let Some(next) = buf.get(index).copied() else {
            break;
        };
        if !(result.wrapping_mul(10) <= high && rule != 0 && next.is_ascii_digit()) {
            break;
        }
        byte = next;
    }
    if result < low || result > high {
        return None;
    }
    *dest = result as i32;
    Some(index)
}

fn find_string(buf: &[u8], names: &[&str]) -> Option<(usize, usize)> {
    for (index, name) in names.iter().enumerate() {
        let bytes = name.as_bytes();
        if bytes.is_empty() {
            continue;
        }
        if let Some(candidate) = buf.get(..bytes.len())
            && candidate.eq_ignore_ascii_case(bytes)
        {
            return Some((index, bytes.len()));
        }
    }
    None
}

fn find_strings(buf: &[u8], full: &[&str], short: &[&str]) -> Option<(usize, usize)> {
    find_string(buf, full).or_else(|| find_string(buf, short))
}

/// Parse `buf` according to `format`; return the unparsed tail or `None`.
pub fn strptime<'a>(
    state: &TzState,
    locale: &dyn TimeLocale,
    buf: &'a [u8],
    format: &[u8],
    tm: &mut Tm,
    ctx: &StrptimeCtx<'_>,
) -> Option<&'a [u8]> {
    parse(state, locale, buf, format, tm, ctx, 0)
}

fn parse<'a>(
    state: &TzState,
    locale: &dyn TimeLocale,
    buf: &'a [u8],
    format: &[u8],
    tm: &mut Tm,
    ctx: &StrptimeCtx<'_>,
    depth: usize,
) -> Option<&'a [u8]> {
    if depth > 8 {
        return None;
    }
    let mut state_bits = 0u32;
    let mut split_year = false;
    let mut day_offset = -1i32;
    let mut week_offset = 0i32;
    let mut bp = 0usize;
    let mut fp = 0usize;
    while let Some(byte) = format.get(fp).copied() {
        fp += 1;
        if is_space(byte) {
            while buf.get(bp).is_some_and(|byte| is_space(*byte)) {
                bp += 1;
            }
            continue;
        }
        if byte != b'%' {
            if buf.get(bp).copied() != Some(byte) {
                return None;
            }
            bp += 1;
            continue;
        }
        let mut alt_e = false;
        let mut alt_o = false;
        loop {
            match format.get(fp).copied() {
                Some(b'E') if !alt_e => {
                    alt_e = true;
                    fp += 1;
                }
                Some(b'O') if !alt_o => {
                    alt_o = true;
                    fp += 1;
                }
                Some(b'E' | b'O') => return None,
                _ => break,
            }
        }
        let conversion = *format.get(fp)?;
        fp += 1;
        let legal = |allowed_e: bool, allowed_o: bool| -> Option<()> {
            if (alt_e && !allowed_e) || (alt_o && !allowed_o) {
                None
            } else {
                Some(())
            }
        };
        match conversion {
            b'%' => {
                legal(false, false)?;
                if buf.get(bp).copied() != Some(b'%') {
                    return None;
                }
                bp += 1;
            }
            b'A' | b'a' => {
                legal(false, false)?;
                let (index, length) = find_strings(
                    buf.get(bp..)?,
                    &[
                        locale.day(0),
                        locale.day(1),
                        locale.day(2),
                        locale.day(3),
                        locale.day(4),
                        locale.day(5),
                        locale.day(6),
                    ],
                    &[
                        locale.abday(0),
                        locale.abday(1),
                        locale.abday(2),
                        locale.abday(3),
                        locale.abday(4),
                        locale.abday(5),
                        locale.abday(6),
                    ],
                )?;
                tm.tm_wday = index as i32;
                bp += length;
                state_bits |= S_WDAY;
            }
            b'B' | b'b' | b'h' => {
                legal(false, false)?;
                let (index, length) = find_strings(
                    buf.get(bp..)?,
                    &[
                        locale.mon(0),
                        locale.mon(1),
                        locale.mon(2),
                        locale.mon(3),
                        locale.mon(4),
                        locale.mon(5),
                        locale.mon(6),
                        locale.mon(7),
                        locale.mon(8),
                        locale.mon(9),
                        locale.mon(10),
                        locale.mon(11),
                    ],
                    &[
                        locale.abmon(0),
                        locale.abmon(1),
                        locale.abmon(2),
                        locale.abmon(3),
                        locale.abmon(4),
                        locale.abmon(5),
                        locale.abmon(6),
                        locale.abmon(7),
                        locale.abmon(8),
                        locale.abmon(9),
                        locale.abmon(10),
                        locale.abmon(11),
                    ],
                )?;
                tm.tm_mon = index as i32;
                bp += length;
                state_bits |= S_MON;
            }
            b'C' => {
                legal(true, false)?;
                let mut value = 20;
                bp += conv_num(buf.get(bp..)?, &mut value, 0, 99)?;
                let century = value * 100 - TM_YEAR_BASE;
                tm.tm_year = if split_year {
                    century + tm.tm_year % 100
                } else {
                    century
                };
                split_year = true;
                state_bits |= S_YEAR;
            }
            b'c' => {
                legal(true, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    locale.d_t_fmt().as_bytes(),
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
                state_bits |= S_WDAY | S_MON | S_MDAY | S_YEAR;
            }
            b'D' => {
                legal(false, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    b"%m/%d/%y",
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
                state_bits |= S_MON | S_MDAY | S_YEAR;
            }
            b'F' => {
                legal(false, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    b"%Y-%m-%d",
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
                state_bits |= S_MON | S_MDAY | S_YEAR;
            }
            b'R' => {
                legal(false, false)?;
                let rest = parse(state, locale, buf.get(bp..)?, b"%H:%M", tm, ctx, depth + 1)?;
                bp = buf.len() - rest.len();
            }
            b'r' => {
                legal(false, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    locale.t_fmt_ampm().as_bytes(),
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
            }
            b'T' => {
                legal(false, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    b"%H:%M:%S",
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
            }
            b'X' => {
                legal(false, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    locale.t_fmt().as_bytes(),
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
            }
            b'x' => {
                legal(false, false)?;
                let rest = parse(
                    state,
                    locale,
                    buf.get(bp..)?,
                    locale.d_fmt().as_bytes(),
                    tm,
                    ctx,
                    depth + 1,
                )?;
                bp = buf.len() - rest.len();
                state_bits |= S_MON | S_MDAY | S_YEAR;
            }
            b'd' | b'e' => {
                legal(false, true)?;
                bp += conv_num(buf.get(bp..)?, &mut tm.tm_mday, 1, 31)?;
                state_bits |= S_MDAY;
            }
            b'k' | b'H' => {
                legal(false, true)?;
                bp += conv_num(buf.get(bp..)?, &mut tm.tm_hour, 0, 23)?;
                state_bits |= S_HOUR;
            }
            b'l' | b'I' => {
                legal(false, true)?;
                let mut value = 0i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 1, 12)?;
                tm.tm_hour = if value == 12 { 0 } else { value };
                state_bits |= S_HOUR;
            }
            b'j' => {
                legal(false, false)?;
                let mut value = 1i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 1, 366)?;
                tm.tm_yday = value - 1;
                state_bits |= S_YDAY;
            }
            b'M' => {
                legal(false, true)?;
                bp += conv_num(buf.get(bp..)?, &mut tm.tm_min, 0, 59)?;
            }
            b'm' => {
                legal(false, true)?;
                let mut value = 1i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 1, 12)?;
                tm.tm_mon = value - 1;
                state_bits |= S_MON;
            }
            b'p' | b'P' => {
                legal(false, false)?;
                let names = [locale.am_pm(0), locale.am_pm(1)];
                let (index, length) = find_string(buf.get(bp..)?, &names)?;
                if (state_bits & S_HOUR) != 0 && tm.tm_hour > 11 {
                    return None;
                }
                tm.tm_hour += index as i32 * 12;
                bp += length;
            }
            b'S' => {
                legal(false, true)?;
                bp += conv_num(buf.get(bp..)?, &mut tm.tm_sec, 0, 61)?;
            }
            b's' => {
                let mut value = 0i64;
                let start = bp;
                let mut index = 0usize;
                while let Some(byte) = buf.get(start + index).copied() {
                    if !byte.is_ascii_digit() {
                        break;
                    }
                    value = value.checked_mul(10)?.checked_add(i64::from(byte - b'0'))?;
                    index += 1;
                }
                if index == 0 {
                    return None;
                }
                let local = ctx.localtime.unwrap_or(localtime_r);
                *tm = local(state, value).ok()?;
                bp += index;
                state_bits |= S_YDAY | S_WDAY | S_MON | S_MDAY | S_YEAR;
            }
            b'U' | b'W' => {
                legal(false, true)?;
                let mut value = 0i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 0, 53)?;
                day_offset = if conversion == b'U' { 0 } else { 1 };
                week_offset = value;
            }
            b'w' => {
                legal(false, true)?;
                bp += conv_num(buf.get(bp..)?, &mut tm.tm_wday, 0, 6)?;
                state_bits |= S_WDAY;
            }
            b'u' => {
                legal(false, true)?;
                let mut value = 0i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 1, 7)?;
                tm.tm_wday = value % 7;
                state_bits |= S_WDAY;
            }
            b'g' => {
                legal(false, false)?;
                let mut value = 0i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 0, 99)?;
            }
            b'G' => {
                legal(false, false)?;
                while buf.get(bp).is_some_and(|byte| byte.is_ascii_digit()) {
                    bp += 1;
                }
            }
            b'V' => {
                legal(false, false)?;
                let mut value = 0i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 1, 53)?;
            }
            b'Y' => {
                legal(true, false)?;
                let mut value = TM_YEAR_BASE;
                bp += conv_num(buf.get(bp..)?, &mut value, 0, 9999)?;
                tm.tm_year = value - TM_YEAR_BASE;
                state_bits |= S_YEAR;
            }
            b'y' => {
                legal(true, true)?;
                let mut value = 0i32;
                bp += conv_num(buf.get(bp..)?, &mut value, 0, 99)?;
                tm.tm_year = if split_year {
                    value + (tm.tm_year / 100) * 100
                } else if value <= 68 {
                    value + 2000 - TM_YEAR_BASE
                } else {
                    value + 1900 - TM_YEAR_BASE
                };
                split_year = true;
                state_bits |= S_YEAR;
            }
            b'Z' | b'z' => {
                let mandatory = conversion == b'z';
                if mandatory {
                    while buf.get(bp).is_some_and(|byte| is_space(*byte)) {
                        bp += 1;
                    }
                }
                let start = bp;
                let (consumed, offset, isdst) = parse_zone(state, buf.get(bp..)?, ctx, mandatory)?;
                if consumed == 0 {
                    bp = start;
                } else {
                    bp += consumed;
                    tm.tm_gmtoff = offset;
                    tm.tm_isdst = isdst;
                    tm.tm_zone = NO_ZONE;
                }
            }
            b'n' | b't' => {
                legal(false, false)?;
                while buf.get(bp).is_some_and(|byte| is_space(*byte)) {
                    bp += 1;
                }
            }
            _ => return None,
        }
    }
    finish(tm, state_bits, day_offset, week_offset);
    buf.get(bp..)
}

fn finish(tm: &mut Tm, mut state: u32, day_offset: i32, week_offset: i32) {
    if (state & S_YDAY) == 0 && (state & S_YEAR) != 0 {
        if (state & S_MON) != 0 && (state & S_MDAY) != 0 {
            let leap = usize::from(isleap_sum(i64::from(tm.tm_year), i64::from(TM_YEAR_BASE)));
            tm.tm_yday = START_OF_MONTH
                .get(leap)
                .and_then(|months| months.get(usize::try_from(tm.tm_mon).ok()?))
                .copied()
                .unwrap_or(0)
                + (tm.tm_mday - 1);
            state |= S_YDAY;
        } else if day_offset != -1 {
            if (state & S_WDAY) == 0 {
                tm.tm_wday = day_offset;
                state |= S_WDAY;
            }
            tm.tm_yday = (7 - first_wday_of(tm.tm_year + TM_YEAR_BASE) + day_offset) % 7
                + (week_offset - 1) * 7
                + tm.tm_wday
                - day_offset;
            state |= S_YDAY;
        }
    }
    if (state & S_YDAY) != 0 && (state & S_YEAR) != 0 {
        if (state & S_MON) == 0 {
            let leap = usize::from(isleap_sum(i64::from(tm.tm_year), i64::from(TM_YEAR_BASE)));
            let months = START_OF_MONTH.get(leap).copied().unwrap_or([0; 13]);
            let mut index = 0usize;
            while tm.tm_yday >= months.get(index).copied().unwrap_or(0) {
                index += 1;
            }
            if index > 12 {
                index = 1;
                tm.tm_yday -= months.get(12).copied().unwrap_or(0);
                tm.tm_year += 1;
            }
            tm.tm_mon = index as i32 - 1;
            state |= S_MON;
        }
        if (state & S_MDAY) == 0 {
            let leap = usize::from(isleap_sum(i64::from(tm.tm_year), i64::from(TM_YEAR_BASE)));
            let months = START_OF_MONTH.get(leap).copied().unwrap_or([0; 13]);
            tm.tm_mday = tm.tm_yday
                - months
                    .get(usize::try_from(tm.tm_mon).ok().unwrap_or(0))
                    .copied()
                    .unwrap_or(0)
                + 1;
            state |= S_MDAY;
        }
        if (state & S_WDAY) == 0 {
            let first = first_wday_of(tm.tm_year + TM_YEAR_BASE);
            tm.tm_wday = (first + tm.tm_yday) % 7;
        }
    }
    let _ = S_HOUR;
}

fn first_wday_of(year: i32) -> i32 {
    let year = i64::from(year);
    let value = 1 + 5 * ((year - 1) % 4) + 4 * ((year - 1) % 100) + 6 * ((year - 1) % 400);
    (value % 7) as i32
}

fn parse_zone(
    state: &TzState,
    buf: &[u8],
    ctx: &StrptimeCtx<'_>,
    mandatory: bool,
) -> Option<(usize, i64, i32)> {
    let first = *buf.first()?;
    if first == b'+' || first == b'-' {
        let negative = first == b'-';
        let mut index = 1usize;
        let mut offset = 0i64;
        let mut digits = 0usize;
        while digits < 4 {
            match buf.get(index).copied() {
                Some(byte) if byte.is_ascii_digit() => {
                    offset = offset * 10 + i64::from(byte - b'0');
                    index += 1;
                    digits += 1;
                }
                Some(b':') if digits == 2 => {
                    index += 1;
                }
                _ => break,
            }
        }
        if buf.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let offset = match digits {
            2 => offset * SECSPERHOUR,
            4 => {
                let minutes = offset % 100;
                if minutes >= SECSPERMIN {
                    return None;
                }
                (offset / 100) * SECSPERHOUR + minutes * SECSPERMIN
            }
            _ => return None,
        };
        if offset >= HOURSPERDAY * SECSPERHOUR {
            return None;
        }
        return Some((index, if negative { -offset } else { offset }, 0));
    }
    if buf.starts_with(b"Z") || buf.starts_with(b"z") {
        return Some((1, 0, 0));
    }
    if buf.starts_with(b"GMT") || buf.starts_with(b"UT") || buf.starts_with(b"UTC") {
        let length = if buf.starts_with(b"GMT") || buf.starts_with(b"UTC") {
            3
        } else {
            2
        };
        if delim(buf.get(length).copied()) {
            return Some((length, 0, 0));
        }
    }
    if let Some((index, length)) = find_string(buf, &NAST) {
        return Some((length, -(5 + index as i64) * SECSPERHOUR, 0));
    }
    if let Some((index, length)) = find_string(buf, &NADT) {
        return Some((length, -(4 + index as i64) * SECSPERHOUR, 1));
    }
    if let Some((index, length)) = find_strings(
        buf,
        &[
            core::str::from_utf8(ctx.tzname[0]).unwrap_or(""),
            core::str::from_utf8(ctx.tzname[1]).unwrap_or(""),
        ],
        &[],
    ) && let Some(info) = state.ttis.iter().find(|info| info.isdst == (index == 1))
    {
        return Some((length, i64::from(info.utoff), i32::from(index == 1)));
    }
    if mandatory { None } else { Some((0, 0, 0)) }
}

#[cfg(test)]
#[path = "strptime_ut.rs"]
mod tests;
