// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-libc-time` oracle: the NetBSD `localtime.c`, `asctime.c`,
//! `difftime.c`, `strftime.c` and `strptime.c` originals.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_long, c_void};

/// The host `struct tm` layout used by the oracle C source.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OracleTm {
    /// Seconds.
    pub tm_sec: c_int,
    /// Minutes.
    pub tm_min: c_int,
    /// Hours.
    pub tm_hour: c_int,
    /// Day of month.
    pub tm_mday: c_int,
    /// Month.
    pub tm_mon: c_int,
    /// Year since 1900.
    pub tm_year: c_int,
    /// Day of week.
    pub tm_wday: c_int,
    /// Day of year.
    pub tm_yday: c_int,
    /// DST flag.
    pub tm_isdst: c_int,
    /// Offset east of UTC.
    pub tm_gmtoff: c_long,
    /// Zone abbreviation pointer.
    pub tm_zone: *const c_char,
}

/// The opaque `timezone_t` handle returned by `tzalloc`.
pub type Timezone = *mut c_void;

#[cfg_attr(
    feature = "oracle-raven-libc-time",
    link(name = "oracle_raven_libc_time", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_tzalloc(name: *const c_char) -> Timezone;
    pub fn oracle_tzfree(zone: Timezone);
    pub fn oracle_localtime_rz(
        zone: Timezone,
        timep: *const i64,
        result: *mut OracleTm,
    ) -> *mut OracleTm;
    pub fn oracle_mktime_z(zone: Timezone, tm: *mut OracleTm) -> i64;
    pub fn oracle_timeoff(tm: *mut OracleTm, offset: c_long) -> i64;
    pub fn oracle_strftime_z(
        zone: Timezone,
        buffer: *mut c_char,
        maxsize: usize,
        format: *const c_char,
        tm: *const OracleTm,
    ) -> usize;
    pub fn oracle_strptime(
        buffer: *const c_char,
        format: *const c_char,
        tm: *mut OracleTm,
    ) -> *mut c_char;
    pub fn oracle_localtime_r(timep: *const i64, result: *mut OracleTm) -> *mut OracleTm;
    pub fn oracle_gmtime_r(timep: *const i64, result: *mut OracleTm) -> *mut OracleTm;
    pub fn oracle_mktime(tm: *mut OracleTm) -> i64;
    pub fn oracle_timegm(tm: *mut OracleTm) -> i64;
    pub fn oracle_tzset();
    pub fn oracle_difftime(t1: i64, t0: i64) -> f64;
    pub fn oracle_asctime_r(tm: *const OracleTm, buffer: *mut c_char) -> *mut c_char;
}
