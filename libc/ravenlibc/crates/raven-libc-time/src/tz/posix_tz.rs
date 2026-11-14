// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
//
// RavenBSD: the POSIX TZ-string parser from
// netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (`getzname` .. `tzparse`). The donor file is in the public domain.

//! POSIX `TZ` string parser.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the donor's parser arithmetic is covered by the vectors.
//!
//! The parser accepts `std offset [dst [offset] [,start[/time],end[/time]]]`,
//! angle-bracket names, and the `Jn`, `n` and `Mm.w.d` rule forms. A DST name
//! without a rule uses the built-in US rule, never `posixrules`.

use alloc::vec::Vec;

use crate::tz::consts::{
    DAYSPERLYEAR, DAYSPERNYEAR, DAYSPERWEEK, EPOCH_YEAR, HOURSPERDAY, MINSPERHOUR, MONSPERYEAR,
    SECSPERDAY, SECSPERHOUR, SECSPERMIN, SECSPERREPEAT, TZ_MAX_TIMES, TZ_MAX_TYPES,
    TZDEFRULESTRING, TZNAME_MAXIMUM, YEARS_OF_OBSERVATIONS, YEARSPERREPEAT,
};
use crate::tz::overflow::{TIME_T_MIN, increment_overflow_time, isleap, year_days};
use crate::tz::{TtInfo, TzError, TzState};

const MONTH_LENGTHS: [[i64; 12]; 2] = [
    [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuleKind {
    JulianDay,
    DayOfYear,
    MonthNthDayOfWeek,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rule {
    kind: RuleKind,
    day: i32,
    week: i32,
    month: i32,
    time: i64,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.pos += 1;
        Some(value)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }
}

fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn getzname<'a>(cursor: &mut Cursor<'a>) -> &'a [u8] {
    let start = cursor.pos;
    while let Some(byte) = cursor.peek() {
        if is_digit(byte) || byte == b',' || byte == b'-' || byte == b'+' {
            break;
        }
        cursor.pos += 1;
    }
    cursor.data.get(start..cursor.pos).unwrap_or(b"")
}

fn getqzname<'a>(cursor: &mut Cursor<'a>, delimiter: u8) -> &'a [u8] {
    let start = cursor.pos;
    while let Some(byte) = cursor.peek() {
        if byte == delimiter {
            break;
        }
        cursor.pos += 1;
    }
    cursor.data.get(start..cursor.pos).unwrap_or(b"")
}

fn getnum(cursor: &mut Cursor<'_>, min: i32, max: i32) -> Result<i32, TzError> {
    let Some(first) = cursor.peek() else {
        return Err(TzError::BadFormat);
    };
    if !is_digit(first) {
        return Err(TzError::BadFormat);
    }
    let mut value = 0i32;
    while let Some(byte) = cursor.peek() {
        if !is_digit(byte) {
            break;
        }
        value = value
            .checked_mul(10)
            .and_then(|current| current.checked_add(i32::from(byte - b'0')))
            .ok_or(TzError::BadFormat)?;
        if value > max {
            return Err(TzError::BadFormat);
        }
        cursor.pos += 1;
    }
    if value < min {
        return Err(TzError::BadFormat);
    }
    Ok(value)
}

fn getsecs(cursor: &mut Cursor<'_>) -> Result<i64, TzError> {
    let mut seconds =
        i64::from(getnum(cursor, 0, (HOURSPERDAY * DAYSPERWEEK - 1) as i32)?) * SECSPERHOUR;
    if cursor.peek() == Some(b':') {
        cursor.pos += 1;
        seconds += i64::from(getnum(cursor, 0, (MINSPERHOUR - 1) as i32)?) * SECSPERMIN;
        if cursor.peek() == Some(b':') {
            cursor.pos += 1;
            seconds += i64::from(getnum(cursor, 0, SECSPERMIN as i32)?);
        }
    }
    Ok(seconds)
}

fn getoffset(cursor: &mut Cursor<'_>) -> Result<i64, TzError> {
    let mut negative = false;
    match cursor.peek() {
        Some(b'-') => {
            negative = true;
            cursor.pos += 1;
        }
        Some(b'+') => cursor.pos += 1,
        _ => {}
    }
    let value = getsecs(cursor)?;
    Ok(if negative { -value } else { value })
}

fn getrule(cursor: &mut Cursor<'_>) -> Result<Rule, TzError> {
    let mut rule = match cursor.peek() {
        Some(b'J') => {
            cursor.pos += 1;
            Rule {
                kind: RuleKind::JulianDay,
                day: getnum(cursor, 1, DAYSPERNYEAR as i32)?,
                week: 0,
                month: 0,
                time: 2 * SECSPERHOUR,
            }
        }
        Some(b'M') => {
            cursor.pos += 1;
            let month = getnum(cursor, 1, MONSPERYEAR as i32)?;
            if cursor.next() != Some(b'.') {
                return Err(TzError::BadFormat);
            }
            let week = getnum(cursor, 1, 5)?;
            if cursor.next() != Some(b'.') {
                return Err(TzError::BadFormat);
            }
            Rule {
                kind: RuleKind::MonthNthDayOfWeek,
                day: getnum(cursor, 0, DAYSPERWEEK as i32 - 1)?,
                week,
                month,
                time: 2 * SECSPERHOUR,
            }
        }
        Some(byte) if is_digit(byte) => Rule {
            kind: RuleKind::DayOfYear,
            day: getnum(cursor, 0, DAYSPERLYEAR as i32 - 1)?,
            week: 0,
            month: 0,
            time: 2 * SECSPERHOUR,
        },
        _ => return Err(TzError::BadFormat),
    };
    if cursor.peek() == Some(b'/') {
        cursor.pos += 1;
        rule.time = getoffset(cursor)?;
    }
    Ok(rule)
}

fn transtime(year: i64, rule: &Rule, offset: i64) -> Result<i64, TzError> {
    let leapyear = isleap(year);
    let day = if rule.kind == RuleKind::MonthNthDayOfWeek {
        let janfeb = rule.month <= 2;
        let month = i64::from(rule.month) + if janfeb { MONSPERYEAR } else { 0 };
        let adjustment = 0 - i64::from(janfeb);
        let ay_rem = (year + adjustment).rem_euclid(YEARSPERREPEAT);
        let y = ay_rem;
        let dow = ((13 * (month + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % DAYSPERWEEK;
        let mut day = i64::from(rule.day) - dow;
        day += (i64::from(rule.week) - i64::from(day >= 0)) * DAYSPERWEEK;
        let months = MONTH_LENGTHS
            .get(usize::from(leapyear))
            .ok_or(TzError::BadFormat)?;
        let month_index = usize::try_from(rule.month - 1).map_err(|_| TzError::BadFormat)?;
        if months.get(month_index).copied().ok_or(TzError::BadFormat)? <= day {
            day -= DAYSPERWEEK;
        }
        for index in 0..month_index {
            day += months.get(index).copied().ok_or(TzError::BadFormat)?;
        }
        day
    } else {
        let mut day = i64::from(rule.day);
        if rule.kind == RuleKind::JulianDay && (!leapyear || rule.day <= 59) {
            day -= 1;
        }
        day
    };
    day.checked_mul(SECSPERDAY)
        .and_then(|seconds| seconds.checked_add(rule.time))
        .and_then(|seconds| seconds.checked_add(offset))
        .ok_or(TzError::BadFormat)
}

/// Parse one POSIX `TZ` string.
///
/// `base` is the state whose leaps and last transition bound the generated
/// transitions; it is used by the TZif footer. Without it the window starts at
/// `TIME_T_MIN`.
pub fn tzparse(name: &[u8], base: Option<&TzState>) -> Result<TzState, TzError> {
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    let name = name.get(..end).unwrap_or(b"");
    if name.len() > TZNAME_MAXIMUM * 2 + 32 {
        return Err(TzError::TooLong);
    }
    let mut cursor = Cursor::new(name);
    let (stdname, stdlen) = parse_name(&mut cursor)?;
    let stdoffset = getoffset(&mut cursor)?;
    let mut state = TzState::empty();
    let mut atlo = TIME_T_MIN;
    if let Some(base) = base {
        state.leapcnt = base.leapcnt;
        state.lsis = base.lsis.clone();
        if let Some(last) = base.last_transition() {
            atlo = last;
        }
    }
    let mut charcnt = stdlen + 1;
    if cursor.at_end() {
        state.typecnt = 1;
        state.timecnt = 0;
        state.charcnt = charcnt;
        state.ttis.push(TtInfo {
            utoff: i32::try_from(-stdoffset).map_err(|_| TzError::BadFormat)?,
            desigidx: 0,
            isdst: false,
        });
        state.chars.extend_from_slice(stdname);
        state.chars.push(0);
        return Ok(state);
    }
    let (dstname, dstlen) = parse_name(&mut cursor)?;
    charcnt += dstlen + 1;
    if charcnt > TZNAME_MAXIMUM + 1 {
        return Err(TzError::TooLong);
    }
    let dstoffset = match cursor.peek() {
        Some(byte) if byte != b',' && byte != b';' => getoffset(&mut cursor)?,
        _ => stdoffset - SECSPERHOUR,
    };
    if cursor.at_end() {
        cursor = Cursor::new(TZDEFRULESTRING);
        cursor.pos += 1;
    } else if cursor.peek() != Some(b',') && cursor.peek() != Some(b';') {
        return Err(TzError::BadFormat);
    } else {
        cursor.pos += 1;
    }
    let start = getrule(&mut cursor)?;
    if cursor.next() != Some(b',') {
        return Err(TzError::BadFormat);
    }
    let stop = getrule(&mut cursor)?;
    if !cursor.at_end() {
        return Err(TzError::BadFormat);
    }
    if state.typecnt + 2 > TZ_MAX_TYPES {
        return Err(TzError::NoMemory);
    }
    state.typecnt = 2;
    state.ttis.push(TtInfo {
        utoff: i32::try_from(-stdoffset).map_err(|_| TzError::BadFormat)?,
        desigidx: 0,
        isdst: false,
    });
    state.ttis.push(TtInfo {
        utoff: i32::try_from(-dstoffset).map_err(|_| TzError::BadFormat)?,
        desigidx: u8::try_from(stdlen + 1).map_err(|_| TzError::TooLong)?,
        isdst: true,
    });
    let mut transitions: Vec<(i64, u8)> = Vec::new();
    let repeatbeg = atlo / SECSPERREPEAT + i64::from(atlo < 0 && atlo % SECSPERREPEAT != 0);
    let mut yearbeg = repeatbeg
        .checked_mul(YEARSPERREPEAT)
        .and_then(|value| value.checked_add(EPOCH_YEAR))
        .ok_or(TzError::BadFormat)?;
    let mut janfirst = repeatbeg
        .checked_mul(SECSPERREPEAT)
        .ok_or(TzError::BadFormat)?;
    while atlo < janfirst {
        let yearsecs = year_days(yearbeg - 1)
            .checked_mul(SECSPERDAY)
            .ok_or(TzError::BadFormat)?;
        let Some(previous) = janfirst.checked_sub(yearsecs) else {
            break;
        };
        janfirst = previous;
        yearbeg -= 1;
    }
    loop {
        let yearsecs = year_days(yearbeg)
            .checked_mul(SECSPERDAY)
            .ok_or(TzError::BadFormat)?;
        let Some(next) = janfirst.checked_add(yearsecs) else {
            break;
        };
        if atlo <= next {
            break;
        }
        yearbeg += 1;
        janfirst = next;
    }
    let yearlim = yearbeg
        .checked_add(YEARS_OF_OBSERVATIONS)
        .ok_or(TzError::BadFormat)?;
    let mut year = yearbeg;
    while year < yearlim {
        let yearsecs = year_days(year)
            .checked_mul(SECSPERDAY)
            .ok_or(TzError::BadFormat)?;
        let mut starttime = transtime(year, &start, stdoffset)?;
        let mut endtime = transtime(year, &stop, dstoffset)?;
        let reversed = endtime < starttime;
        if reversed {
            core::mem::swap(&mut starttime, &mut endtime);
        }
        if reversed || (starttime < endtime && endtime - starttime < yearsecs) {
            let mut at = janfirst;
            if increment_overflow_time(&mut at, starttime) || at < atlo {
                // The transition is outside the requested window.
            } else {
                if transitions.len() >= TZ_MAX_TIMES {
                    return Err(TzError::NoMemory);
                }
                transitions.push((at, u8::from(!reversed)));
            }
            let mut at = janfirst;
            if increment_overflow_time(&mut at, endtime) || at < atlo {
                // The transition is outside the requested window.
            } else {
                if transitions.len() >= TZ_MAX_TIMES {
                    return Err(TzError::NoMemory);
                }
                transitions.push((at, u8::from(reversed)));
            }
        }
        if increment_overflow_time(&mut janfirst, yearsecs) {
            break;
        }
        year += 1;
    }
    let goahead = year.saturating_sub(yearbeg) >= YEARS_OF_OBSERVATIONS;
    state.timecnt = transitions.len();
    if transitions.is_empty() {
        state.ttis.truncate(1);
        state.typecnt = 1;
    } else {
        state.goahead = goahead;
        for (at, kind) in transitions {
            state.ats.push(at);
            state.types.push(kind);
        }
    }
    state.charcnt = charcnt;
    state.chars.extend_from_slice(stdname);
    state.chars.push(0);
    state.chars.extend_from_slice(dstname);
    state.chars.push(0);
    // C70: the generated transition list must be strictly increasing; an extreme rule that
    // steps backwards is a malformed TZ string, not a usable state.
    if state
        .ats
        .windows(2)
        .any(|window| match (window.first(), window.get(1)) {
            (Some(left), Some(right)) => left >= right,
            _ => false,
        })
    {
        return Err(TzError::BadFormat);
    }
    Ok(state)
}

fn parse_name<'a>(cursor: &mut Cursor<'a>) -> Result<(&'a [u8], usize), TzError> {
    let name = if cursor.peek() == Some(b'<') {
        cursor.pos += 1;
        let name = getqzname(cursor, b'>');
        if cursor.next() != Some(b'>') {
            return Err(TzError::BadFormat);
        }
        name
    } else {
        getzname(cursor)
    };
    if name.is_empty() || name.len() > TZNAME_MAXIMUM {
        return Err(TzError::BadFormat);
    }
    Ok((name, name.len()))
}

#[cfg(test)]
#[path = "posix_tz_ut.rs"]
mod tests;

/// Fuzz entry point: parse an arbitrary POSIX TZ string; must never panic.
#[doc(hidden)]
pub fn fuzz_tzparse(data: &[u8]) {
    let _ = tzparse(data, None);
}
