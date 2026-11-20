// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `setitimer` request conversion and range checks.

use super::*;
use raven_libc_abi::errno::EINVAL;

fn value(sec: i64, usec: i64) -> Timeval {
    Timeval {
        tv_sec: sec,
        tv_usec: usec,
    }
}

#[test]
fn itimerval_layout_matches_c() {
    assert_eq!(core::mem::size_of::<ITimerVal>(), 32);
    assert_eq!(core::mem::align_of::<ITimerVal>(), 8);
    assert_eq!(core::mem::offset_of!(ITimerVal, it_interval), 0);
    assert_eq!(core::mem::offset_of!(ITimerVal, it_value), 16);
}

#[test]
fn converts_a_valid_request() {
    let request = ITimerVal {
        it_interval: value(2, 250_000),
        it_value: value(1, 500_000),
    };
    let spec = spec_from(&request).unwrap_or_default();
    assert_eq!(spec.interval.tv_sec, 2);
    assert_eq!(spec.interval.tv_nsec, 250_000_000);
    assert_eq!(spec.value.tv_sec, 1);
    assert_eq!(spec.value.tv_nsec, 500_000_000);
}

#[test]
fn rejects_out_of_range_fields() {
    let bad = [(0, -1), (0, 1_000_000), (-1, 0), (i64::MAX, 0)];
    for (sec, usec) in bad {
        let request = ITimerVal {
            it_interval: value(0, 0),
            it_value: value(sec, usec),
        };
        assert_eq!(spec_from(&request).err(), Some(Errno(EINVAL as u16)));
    }
}

#[test]
fn converts_back_to_timevals() {
    let spec = ITimerSpec {
        interval: Timespec {
            tv_sec: 3,
            tv_nsec: 999_001_000,
        },
        value: Timespec {
            tv_sec: 0,
            tv_nsec: 1_500_000,
        },
    };
    let converted = value_of(spec);
    assert_eq!(converted.it_interval, value(3, 999_001));
    assert_eq!(converted.it_value, value(0, 1_500));
}
