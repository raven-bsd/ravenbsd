// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `wcsftime` tests.

use super::*;
use crate::locale::CLocale;

fn tm() -> Tm {
    Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 1,
        tm_mon: 0,
        tm_year: 70,
        tm_wday: 4,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: crate::tz::NO_ZONE,
    }
}

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(|ch| ch as u32).collect()
}

fn formatted(format: &str, capacity: usize) -> Vec<u32> {
    let state = TzState::utc();
    let format = wide(format);
    let mut out = vec![0u32; capacity];
    let length = wcsftime(
        &state,
        &CLocale,
        &mut out,
        &format,
        &tm(),
        &StrftimeCtx::default(),
    );
    out.get(..length).unwrap_or(&[]).to_vec()
}

#[test]
fn an_ascii_format_matches_strftime() {
    assert_eq!(formatted("%Y-%m-%d", 32), wide("1970-01-01\0"));
    assert_eq!(formatted("%H:%M:%S", 32), wide("00:00:00\0"));
    assert_eq!(formatted("%a %b %e %Y", 32), wide("Thu Jan  1 1970\0"));
}

#[test]
fn a_non_ascii_literal_survives() {
    assert_eq!(formatted("é %Y", 32), wide("é 1970\0"));
}

#[test]
fn a_wide_non_ascii_literal_survives() {
    assert_eq!(formatted("λ %m", 32), wide("λ 01\0"));
}

#[test]
fn an_empty_format_returns_zero() {
    assert_eq!(formatted("", 8), Vec::<u32>::new());
}

#[test]
fn a_small_buffer_returns_zero() {
    assert_eq!(formatted("%Y-%m-%d", 4), Vec::<u32>::new());
}

#[test]
fn a_format_near_the_boundary_fits_exactly() {
    assert_eq!(formatted("%Y", 5), wide("1970\0"));
    assert_eq!(formatted("%Y", 4), Vec::<u32>::new());
}

#[test]
fn the_percent_and_tab_conversions_work() {
    assert_eq!(formatted("%%", 8), wide("%\0"));
    assert_eq!(formatted("a%tb", 8), wide("a\tb\0"));
}

#[test]
fn the_buffer_length_includes_the_nul() {
    let state = TzState::utc();
    let format = wide("%Y");
    let mut out = vec![0u32; 5];
    assert_eq!(
        wcsftime(
            &state,
            &CLocale,
            &mut out,
            &format,
            &tm(),
            &StrftimeCtx::default()
        ),
        5
    );
}

#[test]
fn an_invalid_scalar_in_the_format_returns_zero() {
    let state = TzState::utc();
    let format = [0xD800u32, b'%' as u32, b'Y' as u32];
    let mut out = vec![0u32; 8];
    assert_eq!(
        wcsftime(
            &state,
            &CLocale,
            &mut out,
            &format,
            &tm(),
            &StrftimeCtx::default()
        ),
        0
    );
}
