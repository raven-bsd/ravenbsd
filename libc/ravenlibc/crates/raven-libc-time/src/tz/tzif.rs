// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
//
// RavenBSD: the TZif reader from netbsd:lib/libc/time/localtime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01 (`tzloadbody`), split out. The
// donor file is in the public domain.

//! TZif v1-v4 file reader.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: all offsets and counts are checked before use.
//!
//! The reader is bounded by [`crate::tz::consts`]: a file whose counts exceed the donor's
//! limits is `BadFormat`, never allocated without a bound. The 32-bit block of
//! a v2+ file is skipped; the 64-bit block and the footer TZ string are merged
//! into one state so times after the last explicit transition use the footer.

use alloc::vec;
use alloc::vec::Vec;

use crate::sys::TimeSys;
use crate::tz::consts::{
    CHARS_EXTRA, TZ_CHARS_MAX, TZ_MAGIC, TZ_MAX_CHARS, TZ_MAX_LEAPS, TZ_MAX_TIMES, TZ_MAX_TYPES,
    TZDIR, TZH_CHARCNT, TZH_LEAPCNT, TZH_TIMECNT, TZH_TTISSTDCNT, TZH_TTISUTCNT, TZH_TYPECNT,
    TZH_VERSION, TZHEAD_SIZE,
};
use crate::tz::overflow::increment_overflow_time;
use crate::tz::{LeapInfo, TtInfo, TzError, TzState};

/// `TZ` came from the environment.
pub const TZLOAD_FROMENV: u8 = 1;
/// Merge a footer TZ string.
pub const TZLOAD_TZSTRING: u8 = 2;

/// Maximum TZif bytes accepted from `read_file`.
pub const MAX_TZIF: usize = 64 * 1024;
/// Longest path the loader accepts.
pub const MAX_PATH: usize = 1024;

/// Load a TZif file.
///
/// `name == None` reads `TZDEFAULT`; a leading `:` is removed. The
/// `flags` bits are [`TZLOAD_FROMENV`] (advisory) and [`TZLOAD_TZSTRING`]
/// (merge the footer rule).
pub fn tzload(sys: &dyn TimeSys, name: Option<&[u8]>, flags: u8) -> Result<TzState, TzError> {
    let name = name.unwrap_or(crate::tz::consts::TZDEFAULT);
    let path = zone_path(name, sys.issetugid())?;
    let mut buffer = vec![0u8; MAX_TZIF];
    let length = sys.read_file(&path, &mut buffer).map_err(TzError::Io)?;
    let data = buffer.get(..length).ok_or(TzError::BadFormat)?;
    parse_tzif(data, flags)
}

fn zone_path(name: &[u8], issetugid: bool) -> Result<Vec<u8>, TzError> {
    let mut name = name;
    if name.first() == Some(&b':') {
        name = name.get(1..).unwrap_or(b"");
    }
    if name.is_empty() {
        return Err(TzError::NotFound);
    }
    if name.len() > MAX_PATH {
        return Err(TzError::TooLong);
    }
    let default = crate::tz::consts::TZDEFAULT;
    if name.first() == Some(&b'/') {
        if name == default {
            return Ok(name.to_vec());
        }
        let prefix = [TZDIR, b"/"].concat();
        if name.starts_with(&prefix) {
            return Ok(name.to_vec());
        }
        if issetugid {
            return Err(TzError::Denied);
        }
        return Ok(name.to_vec());
    }
    if contains_parent(name) && issetugid {
        return Err(TzError::Denied);
    }
    let mut path = Vec::with_capacity(TZDIR.len() + 1 + name.len());
    path.extend_from_slice(TZDIR);
    path.push(b'/');
    path.extend_from_slice(name);
    if path.len() > MAX_PATH {
        return Err(TzError::TooLong);
    }
    Ok(path)
}

fn contains_parent(name: &[u8]) -> bool {
    let mut index = 0usize;
    while index + 2 < name.len() {
        if name.get(index) == Some(&b'.')
            && name.get(index + 1) == Some(&b'.')
            && name.get(index + 2) == Some(&b'/')
        {
            return true;
        }
        index += 1;
    }
    name.windows(3).any(|window| window == b"../")
}

#[derive(Clone, Copy, Debug)]
struct Header {
    ttisutcnt: usize,
    ttisstdcnt: usize,
    leapcnt: usize,
    timecnt: usize,
    typecnt: usize,
    charcnt: usize,
}

fn parse_tzif(data: &[u8], flags: u8) -> Result<TzState, TzError> {
    if data.len() < TZHEAD_SIZE || data.get(..4) != Some(TZ_MAGIC) {
        return Err(TzError::BadFormat);
    }
    let version = data.get(TZH_VERSION).copied().unwrap_or(0);
    let mut offset = 0usize;
    let mut state = TzState::empty();
    let mut stored = 4usize;
    while stored <= 8 {
        let header_offset = offset;
        let header = read_header(data, header_offset)?;
        let skip = stored == 4 && version != 0;
        let size = data_block_size(&header, stored)?;
        let body = header_offset
            .checked_add(TZHEAD_SIZE)
            .ok_or(TzError::BadFormat)?;
        let end = body.checked_add(size).ok_or(TzError::BadFormat)?;
        if end > data.len() {
            return Err(TzError::BadFormat);
        }
        if skip {
            offset = end;
            stored = 8;
            continue;
        }
        parse_block(&mut state, data, body, end, &header, stored)?;
        offset = end;
        if version == 0 || stored == 8 {
            break;
        }
        stored = 8;
    }
    if flags & TZLOAD_TZSTRING != 0 {
        merge_footer(&mut state, data, offset)?;
    }
    if state.typecnt == 0 {
        return Err(TzError::BadFormat);
    }
    state.defaulttype = state.ttis.iter().position(|info| !info.isdst).unwrap_or(0);
    state.validate()?;
    Ok(state)
}

fn read_header(data: &[u8], offset: usize) -> Result<Header, TzError> {
    let end = offset.checked_add(TZHEAD_SIZE).ok_or(TzError::BadFormat)?;
    let header = data.get(offset..end).ok_or(TzError::BadFormat)?;
    let count = |at: usize| -> Result<usize, TzError> {
        let bytes = header.get(at..at + 4).ok_or(TzError::BadFormat)?;
        let value = detzcode(bytes)?;
        if value < 0 {
            return Err(TzError::BadFormat);
        }
        Ok(value as usize)
    };
    let header = Header {
        ttisutcnt: count(TZH_TTISUTCNT)?,
        ttisstdcnt: count(TZH_TTISSTDCNT)?,
        leapcnt: count(TZH_LEAPCNT)?,
        timecnt: count(TZH_TIMECNT)?,
        typecnt: count(TZH_TYPECNT)?,
        charcnt: count(TZH_CHARCNT)?,
    };
    if header.leapcnt > TZ_MAX_LEAPS
        || header.typecnt > TZ_MAX_TYPES
        || header.timecnt > TZ_MAX_TIMES
        || header.charcnt > TZ_MAX_CHARS
        || header.ttisstdcnt > TZ_MAX_TYPES
        || header.ttisutcnt > TZ_MAX_TYPES
    {
        return Err(TzError::BadFormat);
    }
    if !(header.ttisstdcnt == header.typecnt || header.ttisstdcnt == 0)
        || !(header.ttisutcnt == header.typecnt || header.ttisutcnt == 0)
    {
        return Err(TzError::BadFormat);
    }
    Ok(header)
}

fn data_block_size(header: &Header, stored: usize) -> Result<usize, TzError> {
    let mut size = header
        .timecnt
        .checked_mul(stored)
        .ok_or(TzError::BadFormat)?;
    size = size
        .checked_add(header.timecnt)
        .and_then(|value| value.checked_add(header.typecnt.checked_mul(6)?))
        .and_then(|value| value.checked_add(header.charcnt))
        .and_then(|value| value.checked_add(header.leapcnt.checked_mul(stored + 4)?))
        .and_then(|value| value.checked_add(header.ttisstdcnt))
        .and_then(|value| value.checked_add(header.ttisutcnt))
        .ok_or(TzError::BadFormat)?;
    Ok(size)
}

fn parse_block(
    state: &mut TzState,
    data: &[u8],
    body: usize,
    end: usize,
    header: &Header,
    stored: usize,
) -> Result<(), TzError> {
    let mut cursor = body;
    let mut times: Vec<i64> = Vec::with_capacity(header.timecnt);
    let mut types: Vec<u8> = Vec::with_capacity(header.timecnt);
    let mut previous = None;
    for _ in 0..header.timecnt {
        let at = read_time(data, cursor, stored)?;
        cursor = cursor.checked_add(stored).ok_or(TzError::BadFormat)?;
        if let Some(last) = previous
            && at < last
        {
            return Err(TzError::BadFormat);
        }
        if previous == Some(at) {
            times.pop();
            types.pop();
        }
        previous = Some(at);
        times.push(at);
        types.push(0);
    }
    for slot in &mut types {
        let value = *data.get(cursor).ok_or(TzError::BadFormat)?;
        cursor = cursor.checked_add(1).ok_or(TzError::BadFormat)?;
        if usize::from(value) >= header.typecnt {
            return Err(TzError::BadFormat);
        }
        *slot = value;
    }
    let mut ttis = Vec::with_capacity(header.typecnt);
    for _ in 0..header.typecnt {
        let utoff = detzcode(data.get(cursor..cursor + 4).ok_or(TzError::BadFormat)?)?;
        cursor = cursor.checked_add(4).ok_or(TzError::BadFormat)?;
        if utoff == i32::MIN {
            return Err(TzError::BadFormat);
        }
        let isdst = *data.get(cursor).ok_or(TzError::BadFormat)?;
        cursor = cursor.checked_add(1).ok_or(TzError::BadFormat)?;
        if isdst > 1 {
            return Err(TzError::BadFormat);
        }
        let desigidx = *data.get(cursor).ok_or(TzError::BadFormat)?;
        cursor = cursor.checked_add(1).ok_or(TzError::BadFormat)?;
        if usize::from(desigidx) >= header.charcnt {
            return Err(TzError::BadFormat);
        }
        ttis.push(TtInfo {
            utoff,
            desigidx,
            isdst: isdst != 0,
        });
    }
    let chars = data
        .get(cursor..cursor + header.charcnt)
        .ok_or(TzError::BadFormat)?;
    cursor = cursor
        .checked_add(header.charcnt)
        .ok_or(TzError::BadFormat)?;
    let mut lsis = Vec::with_capacity(header.leapcnt);
    let mut previous_time = -1i64;
    let mut previous_corr = 0i32;
    for index in 0..header.leapcnt {
        let trans = read_time(data, cursor, stored)?;
        cursor = cursor.checked_add(stored).ok_or(TzError::BadFormat)?;
        let corr = detzcode(data.get(cursor..cursor + 4).ok_or(TzError::BadFormat)?)?;
        cursor = cursor.checked_add(4).ok_or(TzError::BadFormat)?;
        if trans <= previous_time {
            return Err(TzError::BadFormat);
        }
        if index != 0 {
            let delta = corr.saturating_sub(previous_corr);
            if !(-1..=1).contains(&delta) {
                return Err(TzError::BadFormat);
            }
        }
        previous_time = trans;
        previous_corr = corr;
        lsis.push(LeapInfo { trans, corr });
    }
    cursor = cursor
        .checked_add(header.ttisstdcnt)
        .and_then(|value| value.checked_add(header.ttisutcnt))
        .ok_or(TzError::BadFormat)?;
    if cursor > end {
        return Err(TzError::BadFormat);
    }
    state.leapcnt = lsis.len();
    state.timecnt = times.len();
    state.typecnt = ttis.len();
    state.charcnt = header.charcnt;
    state.ats = times;
    state.types = types;
    state.ttis = ttis;
    state.chars.clear();
    state.chars.extend_from_slice(chars);
    state.chars.resize(header.charcnt + CHARS_EXTRA, 0);
    state.lsis = lsis;
    Ok(())
}

fn read_time(data: &[u8], offset: usize, stored: usize) -> Result<i64, TzError> {
    let bytes = data
        .get(offset..offset.checked_add(stored).ok_or(TzError::BadFormat)?)
        .ok_or(TzError::BadFormat)?;
    if stored == 4 {
        detzcode(bytes).map(i64::from)
    } else {
        detzcode64(bytes)
    }
}

fn detzcode(bytes: &[u8]) -> Result<i32, TzError> {
    let bytes = bytes.get(..4).ok_or(TzError::BadFormat)?;
    let first = *bytes.first().ok_or(TzError::BadFormat)?;
    let mut result = i32::from(first & 0x7f);
    for byte in bytes.iter().skip(1) {
        result = result.wrapping_shl(8) | i32::from(*byte);
    }
    if first & 0x80 != 0 {
        result = result.wrapping_add(i32::MIN);
    }
    Ok(result)
}

fn detzcode64(bytes: &[u8]) -> Result<i64, TzError> {
    let bytes = bytes.get(..8).ok_or(TzError::BadFormat)?;
    let first = *bytes.first().ok_or(TzError::BadFormat)?;
    let mut result = i64::from(first & 0x7f);
    for byte in bytes.iter().skip(1) {
        result = result.wrapping_shl(8) | i64::from(*byte);
    }
    if first & 0x80 != 0 {
        result = result.wrapping_add(i64::MIN);
    }
    Ok(result)
}

fn merge_footer(state: &mut TzState, data: &[u8], offset: usize) -> Result<(), TzError> {
    let remaining = data.get(offset..).ok_or(TzError::BadFormat)?;
    if remaining.len() <= 2 || remaining.first() != Some(&b'\n') || remaining.last() != Some(&b'\n')
    {
        return Ok(());
    }
    let body = remaining
        .get(1..remaining.len() - 1)
        .ok_or(TzError::BadFormat)?;
    if body.is_empty() {
        return Ok(());
    }
    let mut footer = crate::tz::posix_tz::tzparse(body, Some(state))?;
    if footer.typecnt == 0 {
        return Ok(());
    }
    if state.typecnt.saturating_add(footer.typecnt) > TZ_MAX_TYPES {
        return Err(TzError::NoMemory);
    }
    reuse_abbreviations(state, &mut footer)?;
    while state.timecnt > 1
        && state.types.get(state.timecnt - 1) == state.types.get(state.timecnt - 2)
    {
        state.timecnt -= 1;
    }
    state.goahead = footer.goahead;
    for index in 0..footer.timecnt {
        let at = footer.ats.get(index).copied().ok_or(TzError::BadFormat)?;
        let mut t = at;
        if increment_overflow_time(&mut t, i64::from(leapcorr(state, at)))
            || (state.timecnt > 0
                && state
                    .ats
                    .get(state.timecnt - 1)
                    .is_some_and(|last| t <= *last))
        {
            continue;
        }
        if state.timecnt >= TZ_MAX_TIMES {
            return Err(TzError::NoMemory);
        }
        state.ats.push(t);
        let kind = footer.types.get(index).copied().ok_or(TzError::BadFormat)?;
        state
            .types
            .push(u8::try_from(state.typecnt + usize::from(kind)).map_err(|_| TzError::NoMemory)?);
        state.timecnt += 1;
    }
    for info in &footer.ttis {
        if state.typecnt >= TZ_MAX_TYPES {
            return Err(TzError::NoMemory);
        }
        state.ttis.push(*info);
        state.typecnt += 1;
    }
    Ok(())
}

fn reuse_abbreviations(state: &mut TzState, footer: &mut TzState) -> Result<(), TzError> {
    let mut charcnt = state.charcnt;
    for info in &mut footer.ttis {
        let start = usize::from(info.desigidx);
        let Some(abbr) = footer.chars.get(start..) else {
            return Err(TzError::BadFormat);
        };
        let end = abbr
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(abbr.len());
        let abbr = abbr.get(..end).ok_or(TzError::BadFormat)?;
        let mut found = None;
        let mut cursor = 0usize;
        while cursor < charcnt {
            let Some(rest) = state.chars.get(cursor..) else {
                break;
            };
            let length = rest
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(rest.len());
            let candidate = rest.get(..length).ok_or(TzError::BadFormat)?;
            if candidate == abbr {
                found = Some(cursor);
                break;
            }
            cursor = cursor
                .checked_add(length)
                .and_then(|value| value.checked_add(1))
                .ok_or(TzError::BadFormat)?;
        }
        let index = if let Some(index) = found {
            index
        } else {
            if charcnt + abbr.len() + 1 > TZ_CHARS_MAX {
                return Err(TzError::NoMemory);
            }
            let index = charcnt;
            state.chars.truncate(charcnt);
            state.chars.extend_from_slice(abbr);
            state.chars.push(0);
            charcnt += abbr.len() + 1;
            index
        };
        info.desigidx = u8::try_from(index).map_err(|_| TzError::NoMemory)?;
    }
    state.charcnt = charcnt;
    state.chars.resize(charcnt + CHARS_EXTRA, 0);
    Ok(())
}

fn leapcorr(state: &TzState, t: i64) -> i32 {
    for leap in state.lsis.iter().rev() {
        if leap.trans <= t {
            return leap.corr;
        }
    }
    0
}

#[cfg(test)]
#[path = "tzif_ut.rs"]
mod tests;

/// Fuzz entry point: parse arbitrary TZif bytes; must never panic.
#[doc(hidden)]
pub fn fuzz_tzload(data: &[u8]) {
    let _ = parse_tzif(data, TZLOAD_TZSTRING);
}
