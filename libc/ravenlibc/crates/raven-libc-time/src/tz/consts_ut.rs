// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TZif constant and header-offset tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn magic_and_header_offsets_are_the_rfc_values() {
    assert_eq!(TZ_MAGIC, b"TZif");
    assert_eq!(TZHEAD_SIZE, 44);
    assert_eq!(TZH_VERSION, 4);
    assert_eq!(TZH_TTISUTCNT, 20);
    assert_eq!(TZH_TTISSTDCNT, 24);
    assert_eq!(TZH_LEAPCNT, 28);
    assert_eq!(TZH_TIMECNT, 32);
    assert_eq!(TZH_TYPECNT, 36);
    assert_eq!(TZH_CHARCNT, 40);
}

#[test]
fn the_donor_limits_are_not_enlarged() {
    assert_eq!(TZ_MAX_TIMES, 2000);
    assert_eq!(TZ_MAX_TYPES, 256);
    assert_eq!(TZ_MAX_CHARS, 256);
    assert_eq!(TZ_MAX_LEAPS, 50);
    assert_eq!(TZ_STRLEN_MAX, 255);
    assert_eq!(TZNAME_MAXIMUM, 254);
    assert_eq!(TZ_CHARS_MAX, 510);
}

#[test]
fn the_second_constants_are_the_donor_values() {
    assert_eq!(SECSPERMIN, 60);
    assert_eq!(MINSPERHOUR, 60);
    assert_eq!(HOURSPERDAY, 24);
    assert_eq!(SECSPERHOUR, 3600);
    assert_eq!(SECSPERDAY, 86_400);
    assert_eq!(DAYSPERWEEK, 7);
    assert_eq!(DAYSPERNYEAR, 365);
    assert_eq!(DAYSPERLYEAR, 366);
    assert_eq!(MONSPERYEAR, 12);
    assert_eq!(YEARSPERREPEAT, 400);
    assert_eq!(DAYSPERREPEAT, 146_097);
    assert_eq!(SECSPERREPEAT, 12_622_780_800);
    assert_eq!(YEARS_OF_OBSERVATIONS, 402);
}

#[test]
fn epoch_constants_match_the_posix_epoch() {
    assert_eq!(EPOCH_YEAR, 1970);
    assert_eq!(EPOCH_WDAY, TM_THURSDAY);
    assert_eq!(TM_YEAR_BASE, 1900);
    assert_eq!(TM_SUNDAY, 0);
    assert_eq!(TM_SATURDAY, 6);
    assert_eq!(TM_JANUARY, 0);
    assert_eq!(TM_DECEMBER, 11);
}
