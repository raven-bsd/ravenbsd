// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `TzCache`, path safety and the fuzz entry points.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::sys::testing::FakeSys;
use crate::tz::consts::TZDIR;
use raven_libc_abi::errno::ENOSYS;

fn utc_file() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"TZif");
    out.push(0);
    out.extend_from_slice(&[0u8; 15]);
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&4u32.to_be_bytes());
    out.extend_from_slice(&0i32.to_be_bytes());
    out.push(0);
    out.push(0);
    out.extend_from_slice(b"UTC\0");
    out
}

#[test]
fn the_cache_starts_at_utc() {
    let cache = TzCache::new();
    assert_eq!(cache.state().ttis[0].utoff, 0);
    assert_eq!(cache.tzname()[0], b"UTC");
    assert_eq!(cache.timezone(), 0);
    assert_eq!(cache.daylight(), 0);
}

#[test]
fn the_cache_reloads_when_tz_changes() {
    let mut cache = TzCache::new();
    assert!(cache.needs_reload(Some(b"UTC0"), 0));
    cache.reload(&FakeSys::epoch(), Some(b"UTC0"), 0);
    assert!(!cache.needs_reload(Some(b"UTC0"), 1));
    assert!(cache.needs_reload(Some(b"EST5"), 2));
    assert!(cache.needs_reload(None, 3));
}

#[test]
fn an_empty_tz_is_utc() {
    let mut cache = TzCache::new();
    cache.reload(&FakeSys::epoch(), Some(b""), 0);
    assert_eq!(cache.state().ttis[0].utoff, 0);
    assert_eq!(cache.tzname()[0], b"UTC");
}

#[test]
fn a_named_zone_is_loaded_from_the_fake_file() {
    let sys = FakeSys::epoch();
    sys.set_file(&utc_file());
    let mut cache = TzCache::new();
    cache.reload(&sys, Some(b"Test/UTC"), 0);
    assert_eq!(cache.state().ttis[0].utoff, 0);
    assert_eq!(cache.state().chars.get(..4), Some(&b"UTC\0"[..]));
}

#[test]
fn a_missing_zone_falls_back_to_utc() {
    let mut cache = TzCache::new();
    cache.reload(&FakeSys::epoch(), Some(b"Test/Missing"), 0);
    assert_eq!(cache.state().ttis[0].utoff, 0);
}

#[test]
fn a_posix_string_wins_over_the_file() {
    let sys = FakeSys::epoch();
    sys.set_file(&utc_file());
    let mut cache = TzCache::new();
    cache.reload(&sys, Some(b"EST5"), 0);
    assert_eq!(cache.state().ttis[0].utoff, -5 * 3600);
}

#[test]
fn an_absolute_tzdir_path_is_allowed() {
    let sys = FakeSys::epoch();
    sys.set_file(&utc_file());
    let path = [TZDIR, b"/Test/UTC"].concat();
    let state = tzif::tzload(&sys, Some(&path), tzif::TZLOAD_TZSTRING).expect("TZDIR path");
    assert_eq!(state.ttis[0].utoff, 0);
}

#[test]
fn a_setuid_absolute_escape_is_denied() {
    let mut sys = FakeSys::epoch();
    sys.setuid = true;
    assert_eq!(
        tzif::tzload(&sys, Some(b"/etc/passwd"), tzif::TZLOAD_TZSTRING),
        Err(TzError::Denied)
    );
}

#[test]
fn a_setuid_parent_escape_is_denied() {
    let mut sys = FakeSys::epoch();
    sys.setuid = true;
    assert_eq!(
        tzif::tzload(&sys, Some(b"../etc/passwd"), tzif::TZLOAD_TZSTRING),
        Err(TzError::Denied)
    );
}

#[test]
fn an_overlong_name_is_too_long() {
    let sys = FakeSys::epoch();
    let name = [b'A'; 2000];
    assert_eq!(
        tzif::tzload(&sys, Some(&name), tzif::TZLOAD_TZSTRING),
        Err(TzError::TooLong)
    );
}

#[test]
fn the_fuzz_entry_points_never_panic_on_short_input() {
    for length in 0..64 {
        let data = [0x5au8; 64];
        tzif::fuzz_tzload(&data[..length]);
        posix_tz::fuzz_tzparse(&data[..length]);
    }
}

#[test]
fn the_state_size_is_below_the_probe_budget() {
    assert!(core::mem::size_of::<TzState>() <= 12 * 1024);
}

#[test]
fn an_unknown_error_maps_to_its_errno() {
    let error = TzError::Io(errno(ENOSYS));
    assert_eq!(error.errno(), errno(ENOSYS));
}

#[test]
fn an_unset_tz_loads_the_default_file() {
    let sys = FakeSys::epoch();
    sys.set_file(&utc_file());
    let mut cache = TzCache::new();
    cache.reload(&sys, None, 0);
    assert_eq!(cache.state().ttis[0].utoff, 0);
    assert!(!cache.needs_reload(None, 1));
}
