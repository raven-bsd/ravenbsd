// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 finding: `mktime` must not panic on an extreme `tm_sec`.

use raven_libc_time::tz::mktime::mktime;
use raven_libc_time::{Tm, TzState};

#[test]
fn extreme_seconds_return_an_error_or_a_value() {
    let mut tm = Tm {
        tm_sec: i32::MIN,
        ..Tm::default()
    };
    let _ = mktime(&TzState::utc(), &mut tm);
}
