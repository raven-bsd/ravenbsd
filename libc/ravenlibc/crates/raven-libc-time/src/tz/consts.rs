// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: constants and header offsets from netbsd:include/tzfile.h @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01.
// The donor file is in the public domain, so clarified as of 1996-06-05 by
// Arthur David Olson.

//! TZif constants and header offsets.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: these are compile-time constants.

/// TZif magic bytes.
pub const TZ_MAGIC: &[u8; 4] = b"TZif";
/// Maximum transition times in one file.
pub const TZ_MAX_TIMES: usize = 2000;
/// Maximum local-time types in one file.
pub const TZ_MAX_TYPES: usize = 256;
/// Maximum abbreviation characters in one file.
pub const TZ_MAX_CHARS: usize = 256;
/// Maximum leap-second records in one file.
pub const TZ_MAX_LEAPS: usize = 50;
/// Maximum bytes in an efficiently handled `TZ` string.
pub const TZ_STRLEN_MAX: usize = 255;
/// Maximum length of one `TZ` abbreviation.
pub const TZNAME_MAXIMUM: usize = 254;
/// Extra bytes after the abbreviation table so `ttunspecified` is safe.
pub const CHARS_EXTRA: usize = 2;
/// Usable size of the abbreviation buffer (the donor's `chars` array).
pub const TZ_CHARS_MAX: usize = 510;

/// Zoneinfo directory.
pub const TZDIR: &[u8] = b"/usr/share/zoneinfo";
/// Default localtime file.
pub const TZDEFAULT: &[u8] = b"/etc/localtime";
/// Built-in US DST rule used when a `TZ` string has no rule.
pub const TZDEFRULESTRING: &[u8] = b",M3.2.0,M11.1.0";

/// Seconds per minute.
pub const SECSPERMIN: i64 = 60;
/// Minutes per hour.
pub const MINSPERHOUR: i64 = 60;
/// Hours per day.
pub const HOURSPERDAY: i64 = 24;
/// Days per week.
pub const DAYSPERWEEK: i64 = 7;
/// Days per non-leap year.
pub const DAYSPERNYEAR: i64 = 365;
/// Days per leap year.
pub const DAYSPERLYEAR: i64 = 366;
/// Seconds per hour.
pub const SECSPERHOUR: i64 = SECSPERMIN * MINSPERHOUR;
/// Seconds per day.
pub const SECSPERDAY: i64 = SECSPERHOUR * HOURSPERDAY;
/// Months per year.
pub const MONSPERYEAR: i64 = 12;
/// Years per Gregorian repeat.
pub const YEARSPERREPEAT: i64 = 400;
/// Days per Gregorian repeat.
pub const DAYSPERREPEAT: i64 = 400 * 365 + 100 - 4 + 1;
/// Seconds per Gregorian repeat.
pub const SECSPERREPEAT: i64 = DAYSPERREPEAT * SECSPERDAY;
/// Average seconds per year over a Gregorian repeat.
pub const AVGSECSPERYEAR: i64 = SECSPERREPEAT / YEARSPERREPEAT;
/// Years of generated transitions kept by the POSIX parser.
pub const YEARS_OF_OBSERVATIONS: i64 = YEARSPERREPEAT + 2;

/// Sunday.
pub const TM_SUNDAY: i32 = 0;
/// Monday.
pub const TM_MONDAY: i32 = 1;
/// Tuesday.
pub const TM_TUESDAY: i32 = 2;
/// Wednesday.
pub const TM_WEDNESDAY: i32 = 3;
/// Thursday.
pub const TM_THURSDAY: i32 = 4;
/// Friday.
pub const TM_FRIDAY: i32 = 5;
/// Saturday.
pub const TM_SATURDAY: i32 = 6;

/// January.
pub const TM_JANUARY: i32 = 0;
/// February.
pub const TM_FEBRUARY: i32 = 1;
/// March.
pub const TM_MARCH: i32 = 2;
/// April.
pub const TM_APRIL: i32 = 3;
/// May.
pub const TM_MAY: i32 = 4;
/// June.
pub const TM_JUNE: i32 = 5;
/// July.
pub const TM_JULY: i32 = 6;
/// August.
pub const TM_AUGUST: i32 = 7;
/// September.
pub const TM_SEPTEMBER: i32 = 8;
/// October.
pub const TM_OCTOBER: i32 = 9;
/// November.
pub const TM_NOVEMBER: i32 = 10;
/// December.
pub const TM_DECEMBER: i32 = 11;

/// The `tm_year` base.
pub const TM_YEAR_BASE: i32 = 1900;
/// The epoch year.
pub const EPOCH_YEAR: i64 = 1970;
/// The epoch weekday (Thursday).
pub const EPOCH_WDAY: i32 = TM_THURSDAY;

/// TZif header, used only for size and offset assertions.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TzHead {
    /// `TZif`.
    pub magic: [u8; 4],
    /// Version byte.
    pub version: [u8; 1],
    /// Reserved bytes.
    pub reserved: [u8; 15],
    /// UT/local indicator count.
    pub ttisutcnt: [u8; 4],
    /// Standard/wall indicator count.
    pub ttisstdcnt: [u8; 4],
    /// Leap-second count.
    pub leapcnt: [u8; 4],
    /// Transition count.
    pub timecnt: [u8; 4],
    /// Local-time type count.
    pub typecnt: [u8; 4],
    /// Abbreviation character count.
    pub charcnt: [u8; 4],
}

/// Size of the TZif header.
pub const TZHEAD_SIZE: usize = 44;
/// Offset of the version byte.
pub const TZH_VERSION: usize = 4;
/// Offset of the UT/local indicator count.
pub const TZH_TTISUTCNT: usize = 20;
/// Offset of the standard/wall indicator count.
pub const TZH_TTISSTDCNT: usize = 24;
/// Offset of the leap-second count.
pub const TZH_LEAPCNT: usize = 28;
/// Offset of the transition count.
pub const TZH_TIMECNT: usize = 32;
/// Offset of the local-time type count.
pub const TZH_TYPECNT: usize = 36;
/// Offset of the abbreviation character count.
pub const TZH_CHARCNT: usize = 40;

const _: () = assert!(core::mem::size_of::<TzHead>() == TZHEAD_SIZE);
const _: () = assert!(core::mem::offset_of!(TzHead, version) == TZH_VERSION);
const _: () = assert!(core::mem::offset_of!(TzHead, ttisutcnt) == TZH_TTISUTCNT);
const _: () = assert!(core::mem::offset_of!(TzHead, ttisstdcnt) == TZH_TTISSTDCNT);
const _: () = assert!(core::mem::offset_of!(TzHead, leapcnt) == TZH_LEAPCNT);
const _: () = assert!(core::mem::offset_of!(TzHead, timecnt) == TZH_TIMECNT);
const _: () = assert!(core::mem::offset_of!(TzHead, typecnt) == TZH_TYPECNT);
const _: () = assert!(core::mem::offset_of!(TzHead, charcnt) == TZH_CHARCNT);
const _: () = assert!(TZ_MAX_TIMES == 2000);
const _: () = assert!(TZ_MAX_TYPES == 256);
const _: () = assert!(TZ_MAX_CHARS == 256);
const _: () = assert!(TZ_MAX_LEAPS == 50);

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
