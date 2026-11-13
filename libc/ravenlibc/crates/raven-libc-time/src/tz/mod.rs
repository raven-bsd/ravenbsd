// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
//
// RavenBSD: the time-zone state model and the reentrant cache from
// netbsd:lib/libc/time/localtime.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01,
// with the lock and process globals moved to the shell. The donor file is in
// the public domain.

//! Time-zone state, the `TZif`/POSIX `TZ` loaders, conversion and `mktime`.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the donor's zone arithmetic is covered by the vectors.

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use crate::sys::{TimeSys, errno};
use consts::{TZ_CHARS_MAX, TZ_MAX_LEAPS, TZ_MAX_TIMES, TZ_MAX_TYPES, TZ_STRLEN_MAX};

pub mod consts;
pub mod convert;
pub mod mktime;
pub mod overflow;
pub mod posix_tz;
pub mod tzif;

/// One local-time type in a TZif file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TtInfo {
    /// UT offset in seconds, east-positive.
    pub utoff: i32,
    /// Offset of the abbreviation in [`TzState::chars`].
    pub desigidx: u8,
    /// Whether the type observes DST.
    pub isdst: bool,
}

/// One leap-second record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeapInfo {
    /// Transition time.
    pub trans: i64,
    /// Cumulative correction.
    pub corr: i32,
}

/// The loaded time-zone state.
///
/// The vectors are bounded by the TZif limits of [`consts`]; a file that
/// exceeds any limit is rejected by the loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TzState {
    /// Number of leap-second records.
    pub leapcnt: usize,
    /// Number of transitions.
    pub timecnt: usize,
    /// Number of local-time types.
    pub typecnt: usize,
    /// Number of abbreviation bytes.
    pub charcnt: usize,
    /// True when the parser knows the rule continues past the generated set.
    pub goahead: bool,
    /// True when the first generated transition is before the requested window.
    pub goback: bool,
    /// Transition times, increasing.
    pub ats: Vec<i64>,
    /// Type index for each transition.
    pub types: Vec<u8>,
    /// Local-time types.
    pub ttis: Vec<TtInfo>,
    /// NUL-terminated abbreviations.
    pub chars: Vec<u8>,
    /// Leap-second records.
    pub lsis: Vec<LeapInfo>,
    /// Type used before the first transition.
    pub defaulttype: usize,
}

impl TzState {
    /// An empty state with no transitions.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            leapcnt: 0,
            timecnt: 0,
            typecnt: 0,
            charcnt: 0,
            goahead: false,
            goback: false,
            ats: Vec::new(),
            types: Vec::new(),
            ttis: Vec::new(),
            chars: Vec::new(),
            lsis: Vec::new(),
            defaulttype: 0,
        }
    }

    /// The built-in UTC state.
    #[must_use]
    pub fn utc() -> Self {
        let mut state = Self::empty();
        state.typecnt = 1;
        state.charcnt = 4;
        state.ttis.push(TtInfo {
            utoff: 0,
            desigidx: 0,
            isdst: false,
        });
        state.chars.extend_from_slice(b"UTC\0");
        state
    }

    /// Return the abbreviation at `zone`.
    #[must_use]
    pub fn abbr(&self, zone: ZoneIdx) -> &[u8] {
        let start = usize::from(zone.0);
        let Some(rest) = self.chars.get(start..) else {
            return b"";
        };
        let end = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());
        rest.get(..end).unwrap_or(b"")
    }

    /// The type index in force at `t`.
    #[must_use]
    pub fn type_index(&self, t: i64) -> usize {
        let mut low = 0usize;
        let mut high = self.timecnt;
        while low < high {
            let middle = low + (high - low) / 2;
            if self.ats.get(middle).is_some_and(|at| *at <= t) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        if low == 0 {
            self.defaulttype.min(self.typecnt.saturating_sub(1))
        } else {
            usize::from(self.types.get(low - 1).copied().unwrap_or(0))
        }
    }

    /// The type in force at `t`.
    #[must_use]
    pub fn ttinfo(&self, t: i64) -> TtInfo {
        self.ttis
            .get(self.type_index(t))
            .copied()
            .unwrap_or(TtInfo {
                utoff: 0,
                desigidx: 0,
                isdst: false,
            })
    }

    /// UT offset in seconds at `t`.
    #[must_use]
    pub fn utoff(&self, t: i64) -> i32 {
        self.ttinfo(t).utoff
    }

    /// Whether DST is in force at `t`.
    #[must_use]
    pub fn isdst(&self, t: i64) -> bool {
        self.ttinfo(t).isdst
    }

    /// Abbreviation at `t`.
    #[must_use]
    pub fn zone_at(&self, t: i64) -> ZoneIdx {
        ZoneIdx(u16::from(self.ttinfo(t).desigidx))
    }

    /// The last transition time, if any.
    #[must_use]
    pub fn last_transition(&self) -> Option<i64> {
        if self.timecnt == 0 {
            None
        } else {
            self.ats.get(self.timecnt - 1).copied()
        }
    }

    /// Keep the state inside the donor's bounds.
    pub(crate) fn validate(&self) -> Result<(), TzError> {
        if self.timecnt > TZ_MAX_TIMES
            || self.typecnt > TZ_MAX_TYPES
            || self.charcnt > TZ_CHARS_MAX
            || self.leapcnt > TZ_MAX_LEAPS
        {
            return Err(TzError::BadFormat);
        }
        Ok(())
    }
}

/// An offset into [`TzState::chars`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZoneIdx(pub u16);

/// The value used when no abbreviation is available.
pub const NO_ZONE: ZoneIdx = ZoneIdx(u16::MAX);

/// A broken-down time.
///
/// `tm_zone` is an offset into the loaded state, not a pointer. The shell
/// materialises the C pointer only for the call that returns it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tm {
    /// Seconds after the minute.
    pub tm_sec: i32,
    /// Minutes after the hour.
    pub tm_min: i32,
    /// Hours since midnight.
    pub tm_hour: i32,
    /// Day of the month.
    pub tm_mday: i32,
    /// Months since January.
    pub tm_mon: i32,
    /// Years since 1900.
    pub tm_year: i32,
    /// Days since Sunday.
    pub tm_wday: i32,
    /// Days since January 1.
    pub tm_yday: i32,
    /// Daylight-saving flag.
    pub tm_isdst: i32,
    /// Seconds east of UTC.
    pub tm_gmtoff: i64,
    /// Abbreviation offset.
    pub tm_zone: ZoneIdx,
}

/// Why a zone could not be loaded or parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TzError {
    /// The TZif file or POSIX string is malformed.
    BadFormat,
    /// The file does not exist.
    NotFound,
    /// A name or file is too long.
    TooLong,
    /// A privileged process tried to escape `TZDIR`.
    Denied,
    /// Allocation failed or a size limit was exceeded.
    NoMemory,
    /// The platform file read failed.
    Io(Errno),
}

impl TzError {
    /// The C-library errno for this failure.
    #[must_use]
    pub const fn errno(self) -> Errno {
        match self {
            Self::BadFormat => errno(raven_libc_abi::errno::EINVAL),
            Self::NotFound => errno(raven_libc_abi::errno::ENOENT),
            Self::TooLong => errno(raven_libc_abi::errno::ENAMETOOLONG),
            Self::Denied => errno(raven_libc_abi::errno::EACCES),
            Self::NoMemory => errno(raven_libc_abi::errno::ENOMEM),
            Self::Io(error) => error,
        }
    }
}

/// The reentrant `tzset` cache.
///
/// The shell owns one of these behind its lock. `needs_reload` is the read
/// phase; `reload` is the write phase. The cache never frees the strings a
/// `Tm::tm_zone` points into, so a value returned by `localtime_r` stays valid
/// until the next `reload`.
pub struct TzCache {
    state: TzState,
    lcl_tzname: [u8; TZ_STRLEN_MAX + 1],
    lcl_len: usize,
    lcl_is_set: bool,
    last_mono: u64,
    tzname: [ZoneIdx; 2],
    timezone: i64,
    daylight: i32,
}

impl Default for TzCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TzCache {
    /// Create a cache holding UTC.
    #[must_use]
    pub fn new() -> Self {
        let state = TzState::utc();
        let mut cache = Self {
            state,
            lcl_tzname: [0; TZ_STRLEN_MAX + 1],
            lcl_len: 0,
            lcl_is_set: false,
            last_mono: 0,
            tzname: [ZoneIdx(0), ZoneIdx(0)],
            timezone: 0,
            daylight: 0,
        };
        cache.update_names();
        cache
    }

    /// Whether the cache must be rebuilt for `tz_env`.
    #[must_use]
    pub fn needs_reload(&self, tz_env: Option<&[u8]>, now_mono: u64) -> bool {
        let _ = now_mono;
        if !self.lcl_is_set {
            return true;
        }
        let wanted = tz_env.unwrap_or(b"");
        if wanted.len() != self.lcl_len {
            return true;
        }
        self.lcl_tzname.get(..self.lcl_len) != Some(wanted)
    }

    /// Rebuild the cache from `tz_env`.
    ///
    /// A missing or malformed zone falls back to UTC, as the donor does.
    pub fn reload(&mut self, sys: &dyn TimeSys, tz_env: Option<&[u8]>, now_mono: u64) {
        self.last_mono = now_mono;
        let name = tz_env.unwrap_or(b"");
        let state = if tz_env.is_none() {
            tzif::tzload(sys, None, tzif::TZLOAD_TZSTRING).unwrap_or_else(|_| TzState::utc())
        } else if name.is_empty() {
            TzState::utc()
        } else {
            match load_zone(sys, name) {
                Ok(state) => state,
                Err(_) => TzState::utc(),
            }
        };
        self.state = state;
        self.lcl_is_set = true;
        self.lcl_len = name.len().min(TZ_STRLEN_MAX);
        self.lcl_tzname.fill(0);
        if let Some(destination) = self.lcl_tzname.get_mut(..self.lcl_len) {
            destination.copy_from_slice(name.get(..self.lcl_len).unwrap_or(b""));
        }
        self.update_names();
    }

    /// The current state.
    #[must_use]
    pub fn state(&self) -> &TzState {
        &self.state
    }

    /// The standard and daylight abbreviations.
    #[must_use]
    pub fn tzname(&self) -> [&[u8]; 2] {
        [
            self.state.abbr(self.tzname[0]),
            self.state.abbr(self.tzname[1]),
        ]
    }

    /// The standard offset west of UTC.
    #[must_use]
    pub const fn timezone(&self) -> i64 {
        self.timezone
    }

    /// Whether the zone observes DST.
    #[must_use]
    pub const fn daylight(&self) -> i32 {
        self.daylight
    }

    fn update_names(&mut self) {
        let mut names = [NO_ZONE, NO_ZONE];
        let mut standard = None;
        let mut daylight = None;
        for (index, info) in self.state.ttis.iter().enumerate() {
            if info.isdst {
                daylight = Some(index);
            } else {
                standard = Some(index);
            }
        }
        if let Some(index) = standard
            && let Some(info) = self.state.ttis.get(index)
        {
            names[0] = ZoneIdx(u16::from(info.desigidx));
            self.timezone = -i64::from(info.utoff);
        } else {
            names[0] = NO_ZONE;
            self.timezone = 0;
        }
        if let Some(index) = daylight
            && let Some(info) = self.state.ttis.get(index)
        {
            names[1] = ZoneIdx(u16::from(info.desigidx));
            self.daylight = 1;
        } else {
            names[1] = names[0];
            self.daylight = 0;
        }
        self.tzname = names;
    }
}

/// Load a zone by name.
///
/// A name beginning with `:` is always a file path. Without it, the POSIX
/// string parser is tried first and the file path second.
pub fn load_zone(sys: &dyn TimeSys, name: &[u8]) -> Result<TzState, TzError> {
    if name.is_empty() {
        return Ok(TzState::utc());
    }
    if name.first() == Some(&b':') {
        let path = name.get(1..).unwrap_or(b"");
        return tzif::tzload(sys, Some(path), tzif::TZLOAD_TZSTRING);
    }
    if let Ok(state) = posix_tz::tzparse(name, None) {
        return Ok(state);
    }
    tzif::tzload(sys, Some(name), tzif::TZLOAD_TZSTRING)
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
