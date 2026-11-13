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

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::locale::TimeLocale;
use crate::tz::consts::{
    DAYSPERNYEAR, DAYSPERWEEK, HOURSPERDAY, MONSPERYEAR, SECSPERMIN, TM_YEAR_BASE,
};
use crate::tz::mktime::timeoff;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pad {
    Default,
    Less,
    Space,
    Zero,
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
    let mut index = 0usize;
    while let Some(byte) = format.get(index).copied() {
        index += 1;
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let mut pad = Pad::Default;
        // RavenBSD: GNU extensions (RD-28): `^` upper-cases the conversion and
        // `#` swaps the case of `%Z`/`%p`.
        let mut upper = false;
        let mut swap = false;
        loop {
            match format.get(index).copied() {
                Some(b'-') => pad = Pad::Less,
                Some(b'_') => pad = Pad::Space,
                Some(b'0') => pad = Pad::Zero,
                Some(b'^') => upper = true,
                Some(b'#') => swap = true,
                Some(b'E' | b'O') => {}
                _ => break,
            }
            index += 1;
        }
        let Some(conversion) = format.get(index).copied() else {
            out.push(b'%');
            break;
        };
        index += 1;
        let mut piece = String::new();
        match conversion {
            b'A' => piece.push_str(day_name(locale, tm.tm_wday, false)),
            b'a' => piece.push_str(day_name(locale, tm.tm_wday, true)),
            b'B' => piece.push_str(month_name(locale, tm.tm_mon, false)),
            b'b' | b'h' => piece.push_str(month_name(locale, tm.tm_mon, true)),
            b'C' => piece.push_str(&yconv(tm.tm_year, TM_YEAR_BASE, true, false)),
            b'c' => {
                let mut nested = Vec::new();
                append_format(
                    state,
                    locale,
                    &mut nested,
                    locale.d_t_fmt().as_bytes(),
                    tm,
                    ctx,
                );
                out.extend_from_slice(&nested);
                continue;
            }
            b'D' => {
                let mut nested = Vec::new();
                append_format(state, locale, &mut nested, b"%m/%d/%y", tm, ctx);
                out.extend_from_slice(&nested);
                continue;
            }
            b'd' => push_num(&mut piece, i64::from(tm.tm_mday), pad, 2, Pad::Zero),
            b'e' => push_num(&mut piece, i64::from(tm.tm_mday), pad, 2, Pad::Space),
            b'F' => {
                let mut nested = Vec::new();
                append_format(state, locale, &mut nested, b"%Y-%m-%d", tm, ctx);
                out.extend_from_slice(&nested);
                continue;
            }
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
            b'R' => {
                let mut nested = Vec::new();
                append_format(state, locale, &mut nested, b"%H:%M", tm, ctx);
                out.extend_from_slice(&nested);
                continue;
            }
            b'r' => {
                let mut nested = Vec::new();
                append_format(
                    state,
                    locale,
                    &mut nested,
                    locale.t_fmt_ampm().as_bytes(),
                    tm,
                    ctx,
                );
                out.extend_from_slice(&nested);
                continue;
            }
            b'S' => push_num(&mut piece, i64::from(tm.tm_sec), pad, 2, Pad::Zero),
            b's' => {
                let mut copy = *tm;
                let value = timeoff(&mut copy, tm.tm_gmtoff).unwrap_or(-1);
                piece.push_str(&format!("{value}"));
            }
            b'T' => {
                let mut nested = Vec::new();
                append_format(state, locale, &mut nested, b"%H:%M:%S", tm, ctx);
                out.extend_from_slice(&nested);
                continue;
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
                push_num(&mut piece, value, pad, 2, Pad::Zero);
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
            b'v' => {
                let mut nested = Vec::new();
                append_format(state, locale, &mut nested, b"%e-%b-%Y", tm, ctx);
                out.extend_from_slice(&nested);
                continue;
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
            b'w' => push_num(&mut piece, i64::from(tm.tm_wday), pad, 2, Pad::Zero),
            b'X' => {
                let mut nested = Vec::new();
                append_format(
                    state,
                    locale,
                    &mut nested,
                    locale.t_fmt().as_bytes(),
                    tm,
                    ctx,
                );
                out.extend_from_slice(&nested);
                continue;
            }
            b'x' => {
                let mut nested = Vec::new();
                append_format(
                    state,
                    locale,
                    &mut nested,
                    locale.d_fmt().as_bytes(),
                    tm,
                    ctx,
                );
                out.extend_from_slice(&nested);
                continue;
            }
            b'y' => piece.push_str(&yconv(tm.tm_year, TM_YEAR_BASE, false, true)),
            b'Y' => piece.push_str(&yconv(tm.tm_year, TM_YEAR_BASE, true, true)),
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
            b'+' => {
                let mut nested = Vec::new();
                append_format(
                    state,
                    locale,
                    &mut nested,
                    b"%a %b %e %H:%M:%S %Z %Y",
                    tm,
                    ctx,
                );
                out.extend_from_slice(&nested);
                continue;
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

fn day_name(locale: &dyn TimeLocale, index: i32, abbreviated: bool) -> &str {
    let Some(index) = usize::try_from(index).ok().filter(|value| *value < 7) else {
        return "?";
    };
    if abbreviated {
        locale.abday(index)
    } else {
        locale.day(index)
    }
}

fn month_name(locale: &dyn TimeLocale, index: i32, abbreviated: bool) -> &str {
    let Some(index) = usize::try_from(index).ok().filter(|value| *value < 12) else {
        return "?";
    };
    if abbreviated {
        locale.abmon(index)
    } else {
        locale.mon(index)
    }
}

fn push_num(out: &mut String, value: i64, pad: Pad, width: usize, default: Pad) {
    let effective = if pad == Pad::Default { default } else { pad };
    let text = match effective {
        Pad::Less => format!("{value}"),
        Pad::Space => format!("{value:>width$}"),
        Pad::Zero | Pad::Default => format!("{value:0width$}"),
    };
    out.push_str(&text);
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
            out.push_str(&format!("{lead:02}"));
        }
    }
    if convert_yy {
        out.push_str(&format!("{:02}", if trail < 0 { -trail } else { trail }));
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
