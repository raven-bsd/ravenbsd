// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 6: the TZif reader and the POSIX `TZ` parser (C44).
//!
//! The body feeds arbitrary bytes to both fuzz entry points and checks the state size, the
//! increasing-transition rule and the TZif limits.

use raven_libc_time::TzState;
use raven_libc_time::tz::posix_tz::{fuzz_tzparse, tzparse};
use raven_libc_time::tz::tzif::fuzz_tzload;

use crate::{Cursor, alloc_probe};

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 6.
pub fn body(data: &[u8]) {
    let cursor = Cursor::new(data);
    let payload = cursor.rest();

    fuzz_tzload(payload);
    fuzz_tzparse(payload);

    assert!(
        core::mem::size_of::<TzState>() <= 12 * 1024,
        "P-tz-2: TzState grew beyond the documented state budget"
    );

    if let Ok(state) = tzparse(payload, None) {
        for window in state.ats.windows(2) {
            if let [left, right] = window {
                assert!(
                    left < right,
                    "P-tz-3: accepted transition times are not strictly increasing"
                );
            }
        }
        assert!(
            state.typecnt <= 256 && state.charcnt <= 510 && state.timecnt <= 2000,
            "P-tz-3: an accepted state exceeds the TZif limits"
        );
    }

    alloc_probe::note(hash(&[
        payload.len() as u64,
        u64::from(payload.first().copied().unwrap_or(0)),
    ]));
}

#[cfg(test)]
#[path = "tz_parse_ut.rs"]
mod tests;
