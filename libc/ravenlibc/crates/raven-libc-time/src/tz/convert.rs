// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
//
// RavenBSD: the conversion half of netbsd:lib/libc/time/localtime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01 (`localsub`, `gmtsub`, `timesub`,
// `leaps_thru_end_of`, `leapcorr`, `time2posix_z`, `posix2time_z`). The donor
// file is in the public domain.

//! `gmtime_r`, `localtime_r`, `offtime` and the POSIX/right conversions.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the donor's floor/year arithmetic is covered by the vectors.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EINVAL, EOVERFLOW};

use crate::sys::errno;
use crate::tz::consts::{
    AVGSECSPERYEAR, DAYSPERLYEAR, DAYSPERNYEAR, DAYSPERREPEAT, DAYSPERWEEK, EPOCH_YEAR, SECSPERDAY,
    SECSPERHOUR, SECSPERMIN, SECSPERREPEAT, TM_YEAR_BASE, YEARSPERREPEAT,
};
use crate::tz::overflow::isleap;
use crate::tz::{NO_ZONE, Tm, TzState};

const MONTH_LENGTHS: [[i32; 12]; 2] = [
    [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
];

const TM_WDAY_BASE: i64 = 1;

/// Convert `t` seconds since the epoch to UTC.
pub fn gmtime_r(t: i64) -> Result<Tm, Errno> {
    let mut tm = timesub(t, 0, None)?;
    tm.tm_zone = NO_ZONE;
    Ok(tm)
}

/// Convert `t` seconds since the epoch in `state`.
pub fn localtime_r(state: &TzState, t: i64) -> Result<Tm, Errno> {
    let (mut tm, year) = localtime_raw(state, t)?;
    tm.tm_year = i32::try_from(year).map_err(|_| errno(EOVERFLOW))?;
    Ok(tm)
}

fn localtime_raw(state: &TzState, t: i64) -> Result<(Tm, i64), Errno> {
    if state.goahead
        && let Some(last) = state.last_transition()
        && last < t
    {
        let tlo = last;
        let diffyears = (t / 2 - tlo / 2 + ((t % 2 - tlo % 2 + 2) / 2 - 1)) / (SECSPERREPEAT / 2)
            * YEARSPERREPEAT;
        let years = diffyears + YEARSPERREPEAT;
        let secs_div_2 = diffyears * (AVGSECSPERYEAR / 2) + SECSPERREPEAT / 2;
        let newt = t - secs_div_2 - secs_div_2;
        let (mut result, year) = localtime_raw(state, newt)?;
        let year = year.checked_add(years).ok_or(errno(EOVERFLOW))?;
        result.tm_year = 0;
        return Ok((result, year));
    }
    let info = state.ttinfo(t);
    let (mut tm, year) = timesub_year(t, i64::from(info.utoff), Some(state))?;
    tm.tm_isdst = i32::from(info.isdst);
    tm.tm_zone = state.zone_at(t);
    Ok((tm, year))
}

/// Convert `t` with a fixed UT offset.
pub fn offtime(t: i64, offset: i64) -> Result<Tm, Errno> {
    let mut tm = timesub(t, offset, None)?;
    tm.tm_zone = NO_ZONE;
    Ok(tm)
}

fn timesub(t: i64, offset: i64, state: Option<&TzState>) -> Result<Tm, Errno> {
    let (mut tm, year) = timesub_year(t, offset, state)?;
    tm.tm_year = i32::try_from(year).map_err(|_| errno(EOVERFLOW))?;
    Ok(tm)
}

fn timesub_year(t: i64, offset: i64, state: Option<&TzState>) -> Result<(Tm, i64), Errno> {
    let mut secs_since_posleap = SECSPERMIN;
    let mut corr = 0i32;
    if let Some(state) = state {
        for index in (0..state.lsis.len()).rev() {
            let leap = state.lsis.get(index).copied().ok_or(errno(EINVAL))?;
            if leap.trans <= t {
                corr = leap.corr;
                let previous = if index == 0 {
                    0
                } else {
                    state.lsis.get(index - 1).map_or(0, |info| info.corr)
                };
                if previous < corr {
                    secs_since_posleap = t - leap.trans;
                }
                break;
            }
        }
    }
    let tdays = t / SECSPERDAY;
    let mut rem = (t % SECSPERDAY) as i32;
    rem += (offset % SECSPERDAY) as i32 - (i64::from(corr) % SECSPERDAY) as i32
        + 3 * SECSPERDAY as i32;
    let dayoff =
        offset / SECSPERDAY - i64::from(corr) / SECSPERDAY + i64::from(rem / SECSPERDAY as i32) - 3;
    rem %= SECSPERDAY as i32;
    let mut dayrem = (tdays % DAYSPERREPEAT) as i32;
    dayrem += (dayoff % DAYSPERREPEAT) as i32;
    let mut y = EPOCH_YEAR - YEARSPERREPEAT
        + (1 + dayoff / DAYSPERREPEAT + i64::from(dayrem / DAYSPERREPEAT as i32)
            - i64::from(dayrem % (DAYSPERREPEAT as i32) < 0)
            + tdays / DAYSPERREPEAT)
            * YEARSPERREPEAT;
    let mut idays = (tdays % DAYSPERREPEAT) as i32;
    idays += (dayoff % DAYSPERREPEAT + 2 * DAYSPERREPEAT) as i32;
    idays %= DAYSPERREPEAT as i32;
    while crate::tz::overflow::year_days(y) <= i64::from(idays) {
        let tdelta = idays / DAYSPERLYEAR as i32;
        let ydelta = tdelta + i32::from(tdelta == 0);
        let newy = y + i64::from(ydelta);
        let leapdays = leaps_thru_end_of(newy - 1) - leaps_thru_end_of(y - 1);
        idays -= ydelta * DAYSPERNYEAR as i32;
        idays -= leapdays as i32;
        y = newy;
    }
    let year = y - i64::from(TM_YEAR_BASE);
    let mut tm = Tm {
        tm_year: 0,
        tm_yday: idays,
        tm_wday: 0,
        tm_hour: 0,
        tm_min: 0,
        tm_sec: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_isdst: 0,
        tm_gmtoff: offset,
        tm_zone: NO_ZONE,
    };
    let wday = TM_WDAY_BASE
        + ((y % DAYSPERWEEK - i64::from(TM_YEAR_BASE) % DAYSPERWEEK) % DAYSPERWEEK)
            * (DAYSPERNYEAR % DAYSPERWEEK)
        + leaps_thru_end_of(y - 1)
        - leaps_thru_end_of(i64::from(TM_YEAR_BASE) - 1)
        + i64::from(idays);
    let mut wday = wday % DAYSPERWEEK;
    if wday < 0 {
        wday += DAYSPERWEEK;
    }
    tm.tm_wday = wday as i32;
    tm.tm_hour = rem / SECSPERHOUR as i32;
    rem %= SECSPERHOUR as i32;
    tm.tm_min = rem / SECSPERMIN as i32;
    tm.tm_sec = rem % SECSPERMIN as i32;
    if secs_since_posleap <= i64::from(tm.tm_sec) {
        tm.tm_sec = tm.tm_sec.saturating_add(1);
    }
    let months = MONTH_LENGTHS
        .get(usize::from(isleap(y)))
        .ok_or(errno(EINVAL))?;
    let mut month = 0usize;
    while idays >= months.get(month).copied().unwrap_or(0) {
        idays -= months.get(month).copied().unwrap_or(0);
        month += 1;
    }
    tm.tm_mon = month as i32;
    tm.tm_mday = idays + 1;
    Ok((tm, year))
}

fn leaps_thru_end_of_nonneg(y: i64) -> i64 {
    y / 4 - y / 100 + y / 400
}

fn leaps_thru_end_of(y: i64) -> i64 {
    if y < 0 {
        -1 - leaps_thru_end_of_nonneg(-1 - y)
    } else {
        leaps_thru_end_of_nonneg(y)
    }
}

/// Leap-second correction in force at `t`.
#[must_use]
pub fn leapcorr(state: &TzState, t: i64) -> i32 {
    for leap in state.lsis.iter().rev() {
        if leap.trans <= t {
            return leap.corr;
        }
    }
    0
}

/// Convert a `right/` time to POSIX time.
pub fn time2posix(state: &TzState, t: i64) -> Result<i64, Errno> {
    t.checked_sub(i64::from(leapcorr(state, t)))
        .ok_or(errno(EOVERFLOW))
}

/// Convert POSIX time to a `right/` time.
pub fn posix2time(state: &TzState, t: i64) -> Result<i64, Errno> {
    t.checked_add(i64::from(leapcorr(state, t)))
        .ok_or(errno(EOVERFLOW))
}

#[cfg(test)]
#[path = "convert_ut.rs"]
mod tests;
