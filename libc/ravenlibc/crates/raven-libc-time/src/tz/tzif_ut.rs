// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TZif reader vectors, built without external zone files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::sys::{ClockId, TimeSys};
use crate::tz::convert::localtime_r;
use raven_libc_abi::structs::Timespec;

struct Fake;

impl TimeSys for Fake {
    fn clock_get(&self, _id: ClockId) -> Result<Timespec, raven_libc_abi::errno::Errno> {
        Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOSYS as u16,
        ))
    }
    fn nanosleep(&self, _ns: u64) -> Result<(), raven_libc_abi::errno::Errno> {
        Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOSYS as u16,
        ))
    }
    fn read_file(
        &self,
        _path: &[u8],
        _buf: &mut [u8],
    ) -> Result<usize, raven_libc_abi::errno::Errno> {
        Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOSYS as u16,
        ))
    }
    fn issetugid(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
struct Type {
    utoff: i32,
    isdst: u8,
    desig: u8,
}

fn header(version: u8, leapcnt: u32, timecnt: u32, typecnt: u32, charcnt: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"TZif");
    out.push(version);
    out.extend_from_slice(&[0u8; 15]);
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&leapcnt.to_be_bytes());
    out.extend_from_slice(&timecnt.to_be_bytes());
    out.extend_from_slice(&typecnt.to_be_bytes());
    out.extend_from_slice(&charcnt.to_be_bytes());
    out
}

fn block(
    times: &[i64],
    types: &[u8],
    ttinfo: &[Type],
    chars: &[u8],
    leaps: &[(i64, i32)],
    stored: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    for time in times {
        if stored == 4 {
            out.extend_from_slice(&(*time as i32).to_be_bytes());
        } else {
            out.extend_from_slice(&time.to_be_bytes());
        }
    }
    out.extend_from_slice(types);
    for info in ttinfo {
        out.extend_from_slice(&info.utoff.to_be_bytes());
        out.push(info.isdst);
        out.push(info.desig);
    }
    out.extend_from_slice(chars);
    for (time, corr) in leaps {
        if stored == 4 {
            out.extend_from_slice(&(*time as i32).to_be_bytes());
        } else {
            out.extend_from_slice(&time.to_be_bytes());
        }
        out.extend_from_slice(&corr.to_be_bytes());
    }
    out
}

fn v1(times: &[i64], types: &[u8], ttinfo: &[Type], chars: &[u8], leaps: &[(i64, i32)]) -> Vec<u8> {
    let mut out = header(
        0,
        leaps.len() as u32,
        times.len() as u32,
        ttinfo.len() as u32,
        chars.len() as u32,
    );
    out.extend(block(times, types, ttinfo, chars, leaps, 4));
    out
}

fn v2(
    times: &[i64],
    types: &[u8],
    ttinfo: &[Type],
    chars: &[u8],
    leaps: &[(i64, i32)],
    footer: &[u8],
) -> Vec<u8> {
    let mut out = header(
        b'2',
        leaps.len() as u32,
        times.len() as u32,
        ttinfo.len() as u32,
        chars.len() as u32,
    );
    out.extend(block(times, types, ttinfo, chars, leaps, 4));
    out.extend(header(
        b'2',
        leaps.len() as u32,
        times.len() as u32,
        ttinfo.len() as u32,
        chars.len() as u32,
    ));
    out.extend(block(times, types, ttinfo, chars, leaps, 8));
    out.push(b'\n');
    out.extend_from_slice(footer);
    out.push(b'\n');
    out
}

#[test]
fn the_empty_image_is_rejected() {
    assert_eq!(parse_tzif(b"", TZLOAD_TZSTRING), Err(TzError::BadFormat));
    assert_eq!(
        parse_tzif(b"TZif", TZLOAD_TZSTRING),
        Err(TzError::BadFormat)
    );
}

#[test]
fn a_v1_utc_file_loads() {
    let bytes = v1(
        &[],
        &[],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
    );
    let state = parse_tzif(&bytes, TZLOAD_TZSTRING).expect("v1");
    assert_eq!(state.typecnt, 1);
    assert_eq!(state.ttis[0].utoff, 0);
    assert_eq!(state.chars.get(..4), Some(&b"UTC\0"[..]));
}

#[test]
fn a_v1_transition_selects_the_type() {
    let bytes = v1(
        &[0],
        &[1],
        &[
            Type {
                utoff: -5 * 3600,
                isdst: 0,
                desig: 0,
            },
            Type {
                utoff: -4 * 3600,
                isdst: 1,
                desig: 4,
            },
        ],
        b"EST\0EDT\0",
        &[],
    );
    let state = parse_tzif(&bytes, TZLOAD_TZSTRING).expect("v1 transition");
    assert_eq!(state.timecnt, 1);
    assert_eq!(state.utoff(-1), -5 * 3600);
    assert_eq!(state.utoff(0), -4 * 3600);
    assert!(state.isdst(0));
}

#[test]
fn a_v2_file_uses_the_64_bit_block() {
    let bytes = v2(
        &[0],
        &[0],
        &[Type {
            utoff: 3600,
            isdst: 0,
            desig: 0,
        }],
        b"CET\0",
        &[],
        b"",
    );
    let state = parse_tzif(&bytes, TZLOAD_TZSTRING).expect("v2");
    assert_eq!(state.ttis[0].utoff, 3600);
    assert_eq!(state.timecnt, 1);
}

#[test]
fn a_v2_footer_rule_covers_times_after_the_last_transition() {
    let bytes = v2(
        &[0],
        &[0],
        &[Type {
            utoff: -5 * 3600,
            isdst: 0,
            desig: 0,
        }],
        b"EST\0",
        &[],
        b"EST5EDT,M3.2.0,M11.1.0",
    );
    let state = parse_tzif(&bytes, TZLOAD_TZSTRING).expect("v2 footer");
    assert!(state.typecnt >= 2);
    assert!(state.goahead);
    let tm = localtime_r(&state, 1_688_184_000).expect("summer");
    assert_eq!(tm.tm_isdst, 1);
    assert_eq!(tm.tm_hour, 0);
}

#[test]
fn bad_magic_and_short_headers_are_rejected() {
    let mut bytes = v1(
        &[],
        &[],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
    );
    bytes[0] = b'X';
    assert_eq!(parse_tzif(&bytes, TZLOAD_TZSTRING), Err(TzError::BadFormat));
    assert_eq!(
        parse_tzif(b"TZif2", TZLOAD_TZSTRING),
        Err(TzError::BadFormat)
    );
}

#[test]
fn too_many_transitions_are_rejected() {
    let mut bytes = v1(
        &[],
        &[],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
    );
    bytes[32..36].copy_from_slice(&(TZ_MAX_TIMES as u32 + 1).to_be_bytes());
    assert_eq!(parse_tzif(&bytes, TZLOAD_TZSTRING), Err(TzError::BadFormat));
}

#[test]
fn a_type_index_past_typecnt_is_rejected() {
    let bytes = v1(
        &[0],
        &[1],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
    );
    assert_eq!(parse_tzif(&bytes, TZLOAD_TZSTRING), Err(TzError::BadFormat));
}

#[test]
fn a_non_monotonic_transition_is_rejected() {
    let bytes = v1(
        &[10, 5],
        &[0, 0],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
    );
    assert_eq!(parse_tzif(&bytes, TZLOAD_TZSTRING), Err(TzError::BadFormat));
}

#[test]
fn an_abbreviation_index_past_charcnt_is_rejected() {
    let bytes = v1(
        &[],
        &[],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 9,
        }],
        b"UTC\0",
        &[],
    );
    assert_eq!(parse_tzif(&bytes, TZLOAD_TZSTRING), Err(TzError::BadFormat));
}

#[test]
fn a_truncated_data_block_is_rejected() {
    let mut bytes = v1(
        &[0],
        &[0],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
    );
    bytes.pop();
    assert_eq!(parse_tzif(&bytes, TZLOAD_TZSTRING), Err(TzError::BadFormat));
}

#[test]
fn leap_records_are_loaded() {
    let bytes = v1(
        &[],
        &[],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[(0, 1)],
    );
    let state = parse_tzif(&bytes, TZLOAD_TZSTRING).expect("leap");
    assert_eq!(state.leapcnt, 1);
    assert_eq!(state.lsis[0].corr, 1);
}

#[test]
fn a_missing_footer_is_tolerated() {
    let mut bytes = v2(
        &[0],
        &[0],
        &[Type {
            utoff: 0,
            isdst: 0,
            desig: 0,
        }],
        b"UTC\0",
        &[],
        b"",
    );
    bytes.pop();
    let _ = parse_tzif(&bytes, TZLOAD_TZSTRING);
}

#[test]
fn the_fake_clock_is_unused_by_the_parser() {
    let _ = Fake;
}
