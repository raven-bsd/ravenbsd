// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 finding: an accepted POSIX `TZ` state has strictly increasing transitions.

use raven_libc_time::tz::posix_tz::tzparse;

#[test]
fn an_extreme_rule_is_rejected_or_increasing() {
    if let Ok(state) = tzparse(b"EST5T,0365,J1/2", None) {
        assert!(state.ats.windows(2).all(|window| window[0] < window[1]));
    }
}
