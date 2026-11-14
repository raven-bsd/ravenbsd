// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 5: `strftime`, `wcsftime`, `strptime`, `mktime`, `timegm`, `asctime_r` (C44).
//!
//! The body feeds arbitrary `Tm` fields, picks one of three zones and checks the output bounds,
//! the suffix contract of `strptime`, the `Ok`/`EOVERFLOW` contract of `mktime` and a round trip
//! of an in-range `Tm`.

use raven_libc_abi::errno::{EINVAL, EOVERFLOW};
use raven_libc_time::asctime::asctime_r;
use raven_libc_time::locale::CLocale;
use raven_libc_time::strftime::{StrftimeCtx, strftime};
use raven_libc_time::strptime::{StrptimeCtx, strptime};
use raven_libc_time::sys::errno;
use raven_libc_time::tz::mktime::{mktime, timegm};
use raven_libc_time::tz::posix_tz::tzparse;
use raven_libc_time::wcsftime::wcsftime;
use raven_libc_time::{Tm, TzState};

use crate::{Cursor, alloc_probe};

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

fn zone(selector: u8) -> TzState {
    match selector % 3 {
        0 => TzState::utc(),
        1 => tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).unwrap_or_else(|_| TzState::utc()),
        _ => tzparse(b"<+0530>-5:30", None).unwrap_or_else(|_| TzState::utc()),
    }
}

fn in_range(tm: &Tm) -> bool {
    (70..=130).contains(&tm.tm_year)
        && (0..=11).contains(&tm.tm_mon)
        && (1..=28).contains(&tm.tm_mday)
        && (0..=23).contains(&tm.tm_hour)
        && (0..=59).contains(&tm.tm_min)
        && (0..=59).contains(&tm.tm_sec)
}

/// Run target 5.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let fields = [
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
        cursor.u32() as i32,
    ];
    let selector = cursor.u8();
    let flags = cursor.u8();
    let rest = cursor.rest();
    let split = rest
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(rest.len().min(128))
        .min(128);
    let format = rest.get(..split).unwrap_or(&[]);
    let subject = rest
        .get(split.saturating_add(1)..)
        .unwrap_or(&[])
        .get(..128)
        .unwrap_or(&[]);
    let state = zone(selector);
    let locale = CLocale;
    let tm = Tm {
        tm_sec: fields[0],
        tm_min: fields[1],
        tm_hour: fields[2],
        tm_mday: fields[3],
        tm_mon: fields[4],
        tm_year: fields[5],
        tm_wday: fields[6],
        tm_yday: fields[7],
        tm_isdst: fields[8],
        ..Tm::default()
    };

    let mut out = [0u8; 256];
    let ctx = StrftimeCtx::default();
    let written = strftime(&state, &locale, &mut out, format, &tm, &ctx);
    assert!(
        written < out.len(),
        "P-time-2: strftime returned a length beyond the buffer"
    );
    if written != 0 {
        assert_eq!(
            out.get(written).copied(),
            Some(0),
            "P-time-2: strftime did not NUL-terminate its result"
        );
    }

    let wide_format: Vec<u32> = format.iter().map(|byte| u32::from(*byte)).collect();
    let mut wide_out = [0u32; 128];
    let _ = wcsftime(&state, &locale, &mut wide_out, &wide_format, &tm, &ctx);

    let mut parsed_tm = tm;
    if let Some(rest) = strptime(
        &state,
        &locale,
        subject,
        format,
        &mut parsed_tm,
        &StrptimeCtx::default(),
    ) {
        let start = subject.as_ptr() as usize;
        let end = start.saturating_add(subject.len());
        let at = rest.as_ptr() as usize;
        assert!(
            at >= start && at <= end,
            "P-time-3: strptime returned a pointer outside the subject"
        );
        assert!(
            rest.len() <= subject.len(),
            "P-time-3: strptime returned a slice longer than the subject"
        );
    }

    let mut local = tm;
    match mktime(&state, &mut local) {
        Ok(_) => {}
        Err(error) => assert!(
            error == errno(EOVERFLOW) || error == errno(EINVAL),
            "P-time-4: mktime returned an undocumented error"
        ),
    }
    let mut utc = tm;
    match timegm(&mut utc) {
        Ok(_) => {}
        Err(error) => assert!(
            error == errno(EOVERFLOW) || error == errno(EINVAL),
            "P-time-4: timegm returned an undocumented error"
        ),
    }
    let mut ascii = [0u8; 26];
    let _ = asctime_r(&tm, &mut ascii);

    if flags & 1 == 0 && in_range(&tm) {
        let mut formatted = [0u8; 64];
        let length = strftime(
            &state,
            &locale,
            &mut formatted,
            b"%Y-%m-%d %H:%M:%S",
            &tm,
            &ctx,
        );
        if length != 0 {
            let mut back = Tm::default();
            if strptime(
                &state,
                &locale,
                formatted.get(..length).unwrap_or(&[]),
                b"%Y-%m-%d %H:%M:%S",
                &mut back,
                &StrptimeCtx::default(),
            )
            .is_some()
            {
                assert!(
                    back.tm_year == tm.tm_year
                        && back.tm_mon == tm.tm_mon
                        && back.tm_mday == tm.tm_mday
                        && back.tm_hour == tm.tm_hour
                        && back.tm_min == tm.tm_min
                        && back.tm_sec == tm.tm_sec,
                    "P-time-5: the formatted timestamp did not round trip"
                );
            }
        }
    }

    alloc_probe::note(hash(&[
        u64::from(written as u32),
        u64::from(selector),
        u64::from(tm.tm_year as u32),
    ]));
}

#[cfg(test)]
#[path = "time_format_ut.rs"]
mod tests;
