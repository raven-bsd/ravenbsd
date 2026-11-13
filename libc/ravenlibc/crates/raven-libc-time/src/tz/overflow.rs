// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the `isleap`, `year_days` and overflow helpers from
// netbsd:lib/libc/time/private.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01,
// partial (i64 time_t only). The donor file is in the public domain.

//! Integer helpers shared by the TZif parser and the calendar code.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: this module exists to make the donor's arithmetic explicit.

/// Minimum `time_t`.
pub const TIME_T_MIN: i64 = i64::MIN;
/// Maximum `time_t`.
pub const TIME_T_MAX: i64 = i64::MAX;

/// Whether `year` is a leap year in the proleptic Gregorian calendar.
#[must_use]
pub const fn isleap(year: i64) -> bool {
    let by_four = year.wrapping_rem(4) == 0;
    let by_hundred = year.wrapping_rem(100) == 0;
    let by_four_hundred = year.wrapping_rem(400) == 0;
    by_four && (!by_hundred || by_four_hundred)
}

/// Leap-year test for two years without overflowing their sum.
#[must_use]
pub const fn isleap_sum(a: i64, b: i64) -> bool {
    isleap(a.wrapping_rem(400).wrapping_add(b.wrapping_rem(400)))
}

/// Days in `year`.
#[must_use]
pub const fn year_days(year: i64) -> i64 {
    365_i64.wrapping_add(isleap(year) as i64)
}

/// Add `delta` to `value`; return `true` when the result does not fit `i32`.
pub fn increment_overflow(value: &mut i32, delta: i64) -> bool {
    let Some(sum) = i64::from(*value).checked_add(delta) else {
        return true;
    };
    if sum < i64::from(i32::MIN) || i64::from(i32::MAX) < sum {
        return true;
    }
    *value = sum as i32;
    false
}

/// Add `delta` to `value`; return `true` when the result does not fit `i64`.
pub fn increment_overflow_time(value: &mut i64, delta: i64) -> bool {
    let Some(sum) = value.checked_add(delta) else {
        return true;
    };
    *value = sum;
    false
}

/// Checked `i32` addition in the shape of C23 `ckd_add`.
#[must_use]
pub fn ckd_add_i32(a: i32, b: i32) -> Option<i32> {
    a.checked_add(b)
}

/// Checked `i64` addition in the shape of C23 `ckd_add`.
#[must_use]
pub fn ckd_add_i64(a: i64, b: i64) -> Option<i64> {
    a.checked_add(b)
}

#[cfg(test)]
#[path = "overflow_ut.rs"]
mod tests;
