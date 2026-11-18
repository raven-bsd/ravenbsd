// SPDX-License-Identifier: BSD-3-Clause
// Copyright 1989 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/time/strftime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS "AS IS" AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! `strftime`: bounded, locale-driven formatting of a broken-down time.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the donor's conversion arithmetic is covered by the vectors.
//!
//! The `%P`, `^` and `#` pieces are the RavenBSD/GNU additions of RD-28 and
//! are marked at their implementation sites. Everything else follows the
//! NetBSD conversion table, including its padding rules and the `%s` copy of
//! the `tm` (A-time-7).

use alloc::string::String;
use alloc::vec::Vec;

use crate::locale::TimeLocale;
use crate::timefmt::{FormatIter, Item, Pad, composite, day_name, month_name};
use crate::tz::consts::{
    DAYSPERNYEAR, DAYSPERWEEK, HOURSPERDAY, MONSPERYEAR, SECSPERMIN, TM_YEAR_BASE,
};
use crate::tz::overflow::isleap;
use crate::tz::{NO_ZONE, Tm, TzState};

/// Context for the conversions that need the zone or a callback.
#[derive(Clone, Copy, Debug)]
pub struct StrftimeCtx<'a> {
    /// The zone's standard and daylight abbreviations.
    pub tzname: [&'a [u8]; 2],
    /// The `localtime_r` callback used by `%s` in `strptime`.
    pub localtime: Option<crate::sys::LocaltimeFn>,
}

impl Default for StrftimeCtx<'_> {
    fn default() -> Self {
        Self {
            tzname: [b"", b""],
            localtime: None,
        }
    }
}

/// Format `tm` into `out`; return the byte count excluding the NUL, or 0.
pub fn strftime(
    state: &TzState,
    locale: &dyn TimeLocale,
    out: &mut [u8],
    format: &[u8],
    tm: &Tm,
    ctx: &StrftimeCtx<'_>,
) -> usize {
    let mut text = Vec::new();
    append_format(state, locale, &mut text, format, tm, ctx);
    if text.len().saturating_add(1) > out.len() {
        return 0;
    }
    if let Some(destination) = out.get_mut(..text.len()) {
        destination.copy_from_slice(&text);
    }
    if let Some(terminator) = out.get_mut(text.len()) {
        *terminator = 0;
    }
    text.len()
}

fn append_format(
    state: &TzState,
    locale: &dyn TimeLocale,
    out: &mut Vec<u8>,
    format: &[u8],
    tm: &Tm,
    ctx: &StrftimeCtx<'_>,
) {
    for item in FormatIter::new(format) {
        let directive = match item {
            Item::Literal(byte) => {
                out.push(byte);
                continue;
            }
            Item::TrailingPercent => {
                out.push(b'%');
                break;
            }
            Item::Directive(directive) => directive,
        };
        let conversion = directive.conversion;
        let pad = directive.pad;
        // RavenBSD: GNU extensions (RD-28): `^` upper-cases the conversion and
        // `#` swaps the case of `%Z`/`%p`.
        let upper = directive.upper;
        let swap = directive.swap;
        if let Some(subformat) = composite(conversion, locale) {
            append_format(state, locale, out, subformat, tm, ctx);
            continue;
        }
        let mut piece = String::new();
        match conversion {
            b'A' => piece.push_str(day_name(locale, tm.tm_wday, false)),
            b'a' => piece.push_str(day_name(locale, tm.tm_wday, true)),
            b'B' => piece.push_str(month_name(locale, tm.tm_mon, false)),
            b'b' | b'h' => piece.push_str(month_name(locale, tm.tm_mon, true)),
            b'C' => {
                // Picolibc/glibc: century = year/100 - (year%100 < 0), width 1.
                let year = i64::from(tm.tm_year) + i64::from(TM_YEAR_BASE);
                let century = year / 100 - i64::from(year % 100 < 0);
                push_num(&mut piece, century, pad, 1, Pad::Zero);
            }
            b'd' => push_num(&mut piece, i64::from(tm.tm_mday), pad, 2, Pad::Zero),
            b'e' => push_num(&mut piece, i64::from(tm.tm_mday), pad, 2, Pad::Space),
            b'H' => push_num(&mut piece, i64::from(tm.tm_hour), pad, 2, Pad::Zero),
            b'I' => {
                let hour = if tm.tm_hour % 12 == 0 {
                    12
                } else {
                    tm.tm_hour % 12
                };
                push_num(&mut piece, i64::from(hour), pad, 2, Pad::Zero);
            }
            b'j' => push_num(&mut piece, i64::from(tm.tm_yday) + 1, pad, 3, Pad::Zero),
            b'k' => push_num(&mut piece, i64::from(tm.tm_hour), pad, 2, Pad::Space),
            b'l' => {
                let hour = if tm.tm_hour % 12 == 0 {
                    12
                } else {
                    tm.tm_hour % 12
                };
                push_num(&mut piece, i64::from(hour), pad, 2, Pad::Space);
            }
            b'M' => push_num(&mut piece, i64::from(tm.tm_min), pad, 2, Pad::Zero),
            b'm' => push_num(&mut piece, i64::from(tm.tm_mon) + 1, pad, 2, Pad::Zero),
            b'n' => piece.push('\n'),
            b'p' => piece.push_str(locale.am_pm(usize::from(tm.tm_hour >= 12))),
            b'P' => {
                // RavenBSD: GNU extension (RD-28): lower-case am/pm.
                piece.push_str(
                    &locale
                        .am_pm(usize::from(tm.tm_hour >= 12))
                        .to_ascii_lowercase(),
                );
            }
            b'S' => push_num(&mut piece, i64::from(tm.tm_sec), pad, 2, Pad::Zero),
            b's' => {
                // glibc's `%s` is `mktime(tm)`: it interprets the broken-down time in the
                // process zone and ignores a caller's `tm_gmtoff`/`tm_zone`.
                let mut copy = *tm;
                let value = crate::tz::mktime::mktime(state, &mut copy).unwrap_or(-1);
                push_i64(&mut piece, value);
            }
            b't' => piece.push('\t'),
            b'U' => {
                let value =
                    (i64::from(tm.tm_yday) + DAYSPERWEEK - i64::from(tm.tm_wday)) / DAYSPERWEEK;
                push_num(&mut piece, value, pad, 2, Pad::Zero);
            }
            b'u' => {
                let value = if tm.tm_wday == 0 {
                    DAYSPERWEEK
                } else {
                    i64::from(tm.tm_wday)
                };
                // The donor formats `%u` with "%d": no padding, regardless of flags.
                push_i64(&mut piece, value);
            }
            b'V' | b'G' | b'g' => {
                let (week, year, base) = iso_week(tm);
                if conversion == b'V' {
                    push_num(&mut piece, week, pad, 2, Pad::Zero);
                } else if conversion == b'g' {
                    piece.push_str(&yconv(year, base, false, true));
                } else {
                    piece.push_str(&yconv(year, base, true, true));
                }
            }
            b'W' => {
                let weekday = if tm.tm_wday != 0 {
                    i64::from(tm.tm_wday - 1)
                } else {
                    DAYSPERWEEK - 1
                };
                let value = (i64::from(tm.tm_yday) + DAYSPERWEEK - weekday) / DAYSPERWEEK;
                push_num(&mut piece, value, pad, 2, Pad::Zero);
            }
            // The donor formats `%w` with "%d": no padding, regardless of flags.
            b'w' => push_i64(&mut piece, i64::from(tm.tm_wday)),
            b'y' => {
                // Picolibc/glibc: tm_year % 100, adjusted into 0..=99.
                let mut year = tm.tm_year % 100;
                if year < 0 {
                    year += 100;
                }
                push_num(&mut piece, i64::from(year), pad, 2, Pad::Zero);
            }
            b'Y' => {
                // Picolibc/glibc: the full year, no minimum width and no zero padding.
                let year = i64::from(tm.tm_year) + i64::from(TM_YEAR_BASE);
                push_i64(&mut piece, year);
            }
            b'Z' => {
                let index = usize::try_from(tm.tm_isdst).ok().filter(|value| *value < 2);
                let zone = if let Some(index) = index
                    && ctx.tzname.get(index).is_some_and(|name| !name.is_empty())
                {
                    ctx.tzname.get(index).copied().unwrap_or(b"")
                } else if tm.tm_zone != NO_ZONE {
                    state.abbr(tm.tm_zone)
                } else {
                    b""
                };
                piece.push_str(core::str::from_utf8(zone).unwrap_or(""));
            }
            b'z' => {
                if tm.tm_isdst < 0 {
                    continue;
                }
                let mut diff = tm.tm_gmtoff;
                let negative = diff < 0;
                if negative {
                    diff = -diff;
                }
                let minutes = diff / SECSPERMIN;
                let value = (minutes / 60) * 100 + (minutes % 60);
                piece.push(if negative { '-' } else { '+' });
                push_num(&mut piece, value, Pad::Default, 4, Pad::Zero);
            }
            b'%' => piece.push('%'),
            _ => {
                out.push(b'%');
                out.push(conversion);
                continue;
            }
        }
        let mut piece = piece.into_bytes();
        if upper {
            piece.make_ascii_uppercase();
        }
        if swap {
            // RavenBSD: GNU extension (RD-28): `#` upper-cases %Z and lower-cases %p.
            if conversion == b'Z' {
                piece.make_ascii_uppercase();
            } else if conversion == b'p' {
                piece.make_ascii_lowercase();
            }
        }
        out.extend_from_slice(&piece);
    }
}

fn push_i64(out: &mut String, value: i64) {
    if value < 0 {
        out.push('-');
    }
    let mut magnitude = value.unsigned_abs();
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();
    loop {
        index -= 1;
        if let Some(slot) = buffer.get_mut(index) {
            *slot = b'0' + (magnitude % 10) as u8;
        } else {
            break;
        }
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    for byte in buffer.get(index..).unwrap_or(&[]) {
        out.push(char::from(*byte));
    }
}

fn push_num(out: &mut String, value: i64, pad: Pad, width: usize, default: Pad) {
    let effective = if pad == Pad::Default { default } else { pad };
    if effective == Pad::Less {
        push_i64(out, value);
        return;
    }
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();
    loop {
        index -= 1;
        if let Some(slot) = buffer.get_mut(index) {
            *slot = b'0' + (magnitude % 10) as u8;
        } else {
            break;
        }
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    let digits = buffer.get(index..).unwrap_or(&[]);
    let total = digits.len() + if negative { 1 } else { 0 };
    if width > total {
        let fill = if effective == Pad::Space { ' ' } else { '0' };
        for _ in 0..width.saturating_sub(total) {
            out.push(fill);
        }
    }
    if negative {
        out.push('-');
    }
    for byte in digits {
        out.push(char::from(*byte));
    }
}

fn yconv(a: i32, b: i32, convert_top: bool, convert_yy: bool) -> String {
    let divisor = 100;
    let mut trail = a % divisor + b % divisor;
    let mut lead = a / divisor + b / divisor + trail / divisor;
    trail %= divisor;
    if trail < 0 && lead > 0 {
        trail += divisor;
        lead -= 1;
    } else if lead < 0 && trail > 0 {
        trail -= divisor;
        lead += 1;
    }
    let mut out = String::new();
    if convert_top {
        if lead == 0 && trail < 0 {
            out.push_str("-0");
        } else {
            push_num(&mut out, i64::from(lead), Pad::Zero, 2, Pad::Zero);
        }
    }
    if convert_yy {
        push_num(
            &mut out,
            i64::from(if trail < 0 { -trail } else { trail }),
            Pad::Zero,
            2,
            Pad::Zero,
        );
    }
    out
}

fn year_sum_days(year: i32, base: i32) -> i32 {
    DAYSPERNYEAR as i32 + i32::from(isleap(i64::from(year % 400) + i64::from(base % 400)))
}

fn iso_week(tm: &Tm) -> (i64, i32, i32) {
    let year = tm.tm_year;
    let mut base = TM_YEAR_BASE;
    let mut yday = i64::from(tm.tm_yday);
    let wday = i64::from(tm.tm_wday);
    loop {
        let len = year_sum_days(year, base);
        let bot = ((yday + 11 - wday) % DAYSPERWEEK) - 3;
        let mut top = bot - (i64::from(len) % DAYSPERWEEK);
        if top < -3 {
            top += DAYSPERWEEK;
        }
        top += i64::from(len);
        if yday >= top {
            base += 1;
            return (1, year, base);
        }
        if yday >= bot {
            return (1 + (yday - bot) / DAYSPERWEEK, year, base);
        }
        base -= 1;
        yday += i64::from(year_sum_days(year, base));
    }
}

const _: () = assert!(MONSPERYEAR == 12);
const _: () = assert!(HOURSPERDAY == 24);

#[cfg(test)]
#[path = "strftime_ut.rs"]
mod tests;
