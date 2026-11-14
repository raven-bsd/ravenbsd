// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
//
// RavenBSD: the `mktime` search from netbsd:lib/libc/time/localtime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01 (`time2sub`, `time2`, `time1`,
// `mktime_tzname`, `timegm`, `timeoff`). The donor file is in the public
// domain.

//! `mktime`, `timegm` and `timeoff`.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the donor's normalisation arithmetic is covered by the vectors.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EINVAL, EOVERFLOW};

use crate::sys::errno;
use crate::tz::consts::{
    DAYSPERLYEAR, EPOCH_YEAR, HOURSPERDAY, MINSPERHOUR, MONSPERYEAR, SECSPERMIN, TM_YEAR_BASE,
    TZ_MAX_TYPES,
};
use crate::tz::convert::{localtime_r, offtime};
use crate::tz::overflow::{TIME_T_MAX, TIME_T_MIN, isleap, year_days};
use crate::tz::{Tm, TzState};

const MONTH_LENGTHS: [[i32; 12]; 2] = [
    [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
];

#[derive(Clone, Copy)]
enum Zone<'a> {
    Local(&'a TzState),
    Fixed(i64),
}

fn zone_time(zone: Zone<'_>, t: i64) -> Result<Tm, Errno> {
    match zone {
        Zone::Local(state) => localtime_r(state, t),
        Zone::Fixed(offset) => offtime(t, offset),
    }
}

fn tmcomp(a: &Tm, b: &Tm) -> i32 {
    if a.tm_year != b.tm_year {
        return if a.tm_year < b.tm_year { -1 } else { 1 };
    }
    if a.tm_mon != b.tm_mon {
        return if a.tm_mon < b.tm_mon { -1 } else { 1 };
    }
    if a.tm_mday != b.tm_mday {
        return if a.tm_mday < b.tm_mday { -1 } else { 1 };
    }
    if a.tm_hour != b.tm_hour {
        return if a.tm_hour < b.tm_hour { -1 } else { 1 };
    }
    if a.tm_min != b.tm_min {
        return if a.tm_min < b.tm_min { -1 } else { 1 };
    }
    match a.tm_sec.cmp(&b.tm_sec) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

fn time2sub(zone: Zone<'_>, tmp: &mut Tm, do_norm_secs: bool) -> Result<i64, Errno> {
    let mut yourtm = *tmp;
    let mut min = i64::from(yourtm.tm_min);
    if do_norm_secs {
        min += i64::from(yourtm.tm_sec) / SECSPERMIN;
        yourtm.tm_sec = (i64::from(yourtm.tm_sec) % SECSPERMIN) as i32;
        if yourtm.tm_sec < 0 {
            yourtm.tm_sec += SECSPERMIN as i32;
            min -= 1;
        }
    }
    let mut hour = i64::from(yourtm.tm_hour);
    hour += min / MINSPERHOUR;
    yourtm.tm_min = (min % MINSPERHOUR) as i32;
    if yourtm.tm_min < 0 {
        yourtm.tm_min += MINSPERHOUR as i32;
        hour -= 1;
    }
    let mut mday = i64::from(yourtm.tm_mday);
    mday += hour / HOURSPERDAY;
    yourtm.tm_hour = (hour % HOURSPERDAY) as i32;
    if yourtm.tm_hour < 0 {
        yourtm.tm_hour += HOURSPERDAY as i32;
        mday -= 1;
    }
    let mut y = i64::from(yourtm.tm_year);
    y += i64::from(yourtm.tm_mon) / MONSPERYEAR;
    yourtm.tm_mon = (i64::from(yourtm.tm_mon) % MONSPERYEAR) as i32;
    if yourtm.tm_mon < 0 {
        yourtm.tm_mon += MONSPERYEAR as i32;
        y -= 1;
    }
    y += i64::from(TM_YEAR_BASE);
    while mday <= 0 {
        let leap_year = y - i64::from(yourtm.tm_mon <= 1);
        mday += year_days(leap_year);
        y -= 1;
    }
    while DAYSPERLYEAR < mday {
        let leap_year = y + i64::from(1 < yourtm.tm_mon);
        mday -= year_days(leap_year);
        y += 1;
    }
    yourtm.tm_mday = i32::try_from(mday).map_err(|_| errno(EOVERFLOW))?;
    loop {
        let month_length = MONTH_LENGTHS
            .get(usize::from(isleap(y)))
            .and_then(|months| months.get(usize::try_from(yourtm.tm_mon).ok()?))
            .copied()
            .ok_or(errno(EOVERFLOW))?;
        if yourtm.tm_mday <= month_length {
            break;
        }
        yourtm.tm_mday -= month_length;
        yourtm.tm_mon += 1;
        if yourtm.tm_mon >= MONSPERYEAR as i32 {
            yourtm.tm_mon = 0;
            y += 1;
        }
    }
    let year = y - i64::from(TM_YEAR_BASE);
    yourtm.tm_year = i32::try_from(year).map_err(|_| errno(EOVERFLOW))?;
    let saved_seconds = if (0..SECSPERMIN).contains(&i64::from(yourtm.tm_sec)) {
        0
    } else if yourtm.tm_year < EPOCH_YEAR as i32 - TM_YEAR_BASE {
        let saved = yourtm.tm_sec;
        yourtm.tm_sec = SECSPERMIN as i32 - 1;
        // C70: the donor relies on two's-complement wrap for an extreme tm_sec; keep that
        // behaviour without a debug-build overflow panic.
        saved.wrapping_sub(SECSPERMIN as i32 - 1)
    } else {
        let saved = yourtm.tm_sec;
        yourtm.tm_sec = 0;
        saved
    };
    let mut lo = TIME_T_MIN;
    let mut hi = TIME_T_MAX;
    let mut t;
    loop {
        t = lo / 2 + hi / 2;
        if t < lo {
            t = lo;
        } else if t > hi {
            t = hi;
        }
        let dir = match zone_time(zone, t) {
            Ok(mytm) => tmcomp(&mytm, &yourtm),
            Err(_) => {
                if t > 0 {
                    1
                } else {
                    -1
                }
            }
        };
        if dir == 0 {
            let mytm = zone_time(zone, t)?;
            if yourtm.tm_isdst < 0 || mytm.tm_isdst == yourtm.tm_isdst {
                let final_t = t
                    .checked_add(i64::from(saved_seconds))
                    .ok_or(errno(EOVERFLOW))?;
                *tmp = zone_time(zone, final_t)?;
                return Ok(final_t);
            }
            // Right time, wrong type: try the other offset, then apply the
            // saved seconds exactly as the donor's `label:` path does.
            let adjusted = adjust_isdst(zone, t, &yourtm)?;
            let final_t = adjusted
                .checked_add(i64::from(saved_seconds))
                .ok_or(errno(EOVERFLOW))?;
            *tmp = zone_time(zone, final_t)?;
            return Ok(final_t);
        }
        if t == lo {
            if t == TIME_T_MAX {
                return Err(errno(EOVERFLOW));
            }
            let next = t + 1;
            t = next;
            lo = next;
        } else if t == hi {
            if t == TIME_T_MIN {
                return Err(errno(EOVERFLOW));
            }
            let next = t - 1;
            t = next;
            hi = next;
        }
        if lo > hi {
            return Err(errno(EINVAL));
        }
        if dir > 0 {
            hi = t;
        } else {
            lo = t;
        }
    }
}

fn adjust_isdst(zone: Zone<'_>, t: i64, yourtm: &Tm) -> Result<i64, Errno> {
    let state = match zone {
        Zone::Local(state) => state,
        Zone::Fixed(_) => return Err(errno(EINVAL)),
    };
    let wanted = yourtm.tm_isdst;
    for same in 0..state.typecnt {
        let same_info = state.ttis.get(same).copied().ok_or(errno(EINVAL))?;
        if (if same_info.isdst { 1 } else { 0 }) != wanted {
            continue;
        }
        for other in 0..state.typecnt {
            let other_info = state.ttis.get(other).copied().ok_or(errno(EINVAL))?;
            if other_info.isdst == same_info.isdst {
                continue;
            }
            let delta = i64::from(other_info.utoff) - i64::from(same_info.utoff);
            let newt = t.checked_add(delta).ok_or(errno(EOVERFLOW))?;
            if let Ok(mytm) = localtime_r(state, newt)
                && tmcomp(&mytm, yourtm) == 0
                && mytm.tm_isdst == wanted
            {
                return Ok(newt);
            }
        }
    }
    Err(errno(EOVERFLOW))
}

/// `time2`: the first pass without seconds normalisation, then with it.
fn time2(zone: Zone<'_>, tmp: &mut Tm) -> Result<i64, Errno> {
    match time2sub(zone, tmp, false) {
        Ok(t) => Ok(t),
        Err(_) => time2sub(zone, tmp, true),
    }
}

/// `time1`: the donor's final fallback for a `tm_isdst`/offset mismatch.
///
/// When `time2` cannot match the requested type, the donor tries each
/// transition type pair, shifts `tm_sec` by the UT-offset difference and
/// toggles `tm_isdst` before searching again.
fn time1(zone: Zone<'_>, tmp: &mut Tm) -> Result<i64, Errno> {
    let failure = match time2(zone, tmp) {
        Ok(t) => return Ok(t),
        Err(error) => error,
    };
    if tmp.tm_isdst < 0 {
        return Err(failure);
    }
    let state = match zone {
        Zone::Local(state) => state,
        Zone::Fixed(_) => return Err(errno(EINVAL)),
    };
    let mut seen = [false; TZ_MAX_TYPES];
    let mut types = [0usize; TZ_MAX_TYPES];
    let mut nseen = 0usize;
    for kind in state.types.iter().rev() {
        let index = usize::from(*kind);
        if index < state.typecnt && !seen.get(index).copied().unwrap_or(true) {
            if let Some(slot) = seen.get_mut(index) {
                *slot = true;
            }
            if let Some(slot) = types.get_mut(nseen) {
                *slot = index;
                nseen += 1;
            }
        }
    }
    for sameind in 0..nseen {
        let Some(&same_index) = types.get(sameind) else {
            continue;
        };
        let Some(same_info) = state.ttis.get(same_index).copied() else {
            continue;
        };
        if (if same_info.isdst { 1 } else { 0 }) != tmp.tm_isdst {
            continue;
        }
        for otherind in 0..nseen {
            let Some(&other_index) = types.get(otherind) else {
                continue;
            };
            let Some(other_info) = state.ttis.get(other_index).copied() else {
                continue;
            };
            if other_info.isdst == same_info.isdst {
                continue;
            }
            let saved_seconds = tmp.tm_sec;
            let delta = i64::from(other_info.utoff) - i64::from(same_info.utoff);
            let Some(seconds) = i64::from(saved_seconds).checked_add(delta) else {
                continue;
            };
            let Ok(seconds) = i32::try_from(seconds) else {
                continue;
            };
            tmp.tm_sec = seconds;
            tmp.tm_isdst = 1 - tmp.tm_isdst;
            if let Ok(t) = time2(zone, tmp) {
                return Ok(t);
            }
            tmp.tm_isdst = 1 - tmp.tm_isdst;
            tmp.tm_sec = saved_seconds;
        }
    }
    Err(errno(EOVERFLOW))
}

/// Convert a broken-down local time in `state` to seconds since the epoch.
pub fn mktime(state: &TzState, tm: &mut Tm) -> Result<i64, Errno> {
    if tm.tm_isdst > 1 {
        tm.tm_isdst = 1;
    }
    time1(Zone::Local(state), tm)
}

/// Convert a broken-down UTC time to seconds since the epoch.
pub fn timegm(tm: &mut Tm) -> Result<i64, Errno> {
    let mut copy = *tm;
    let t = timeoff(&mut copy, 0)?;
    *tm = copy;
    Ok(t)
}

/// Convert a broken-down time with a fixed offset to seconds since the epoch.
pub fn timeoff(tm: &mut Tm, offset: i64) -> Result<i64, Errno> {
    tm.tm_isdst = 0;
    time1(Zone::Fixed(offset), tm)
}

#[cfg(test)]
#[path = "mktime_ut.rs"]
mod tests;
