// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `times`.

use super::*;

#[test]
fn conversion_matches_the_donor_example() {
    assert_eq!(
        convtck(
            Timeval {
                sec: 3,
                usec: 999_999
            },
            100
        ),
        Ok(399)
    );
}

#[test]
fn zero_converts_to_zero() {
    assert_eq!(convtck(Timeval { sec: 0, usec: 0 }, 100), Ok(0));
}

#[test]
fn a_zero_clock_is_invalid() {
    assert_eq!(
        convtck(Timeval { sec: 1, usec: 0 }, 0),
        Err(crate::errors::EINVAL)
    );
}

#[test]
fn a_clock_of_two_converts_microseconds() {
    assert_eq!(
        convtck(
            Timeval {
                sec: 2,
                usec: 500_000
            },
            2
        ),
        Ok(5)
    );
}

#[test]
fn times_fills_the_record_and_returns_monotonic_ticks() {
    let mut tms = Tms::default();
    let result = times(
        Some(&mut tms),
        Timeval { sec: 1, usec: 0 },
        Timeval { sec: 2, usec: 0 },
        Timeval { sec: 3, usec: 0 },
        Timeval { sec: 4, usec: 0 },
        Timeval { sec: 5, usec: 0 },
        100,
    );
    assert_eq!(result, Ok(500));
    assert_eq!(tms.tms_utime, 100);
    assert_eq!(tms.tms_stime, 200);
    assert_eq!(tms.tms_cutime, 300);
    assert_eq!(tms.tms_cstime, 400);
}

#[test]
fn a_null_record_still_returns_the_clock() {
    let result = times(
        None,
        Timeval { sec: 0, usec: 0 },
        Timeval { sec: 0, usec: 0 },
        Timeval { sec: 0, usec: 0 },
        Timeval { sec: 0, usec: 0 },
        Timeval { sec: 7, usec: 0 },
        100,
    );
    assert_eq!(result, Ok(700));
}

#[test]
fn overflow_is_reported() {
    assert_eq!(
        convtck(
            Timeval {
                sec: i64::MAX,
                usec: 0
            },
            100
        ),
        Err(crate::errors::EOVERFLOW)
    );
}
