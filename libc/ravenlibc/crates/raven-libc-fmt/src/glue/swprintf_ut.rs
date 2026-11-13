// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::{FakeFloat, SliceVa};

fn wide(fmt: &str, text: &str, cap: usize) -> (Result<i32, FmtErr>, Vec<u32>) {
    let mut va = SliceVa::new();
    let p = va.cstr(text);
    va.ptr(p);
    let mut out = vec![0xAAAAu32; cap.saturating_add(4)];
    let units: Vec<u32> = fmt.chars().map(|c| c as u32).collect();
    let opts = FmtOptions::default();
    let r = vswprintf_core(&mut out, cap, &units, &mut va, &opts, &FakeFloat);
    (r, out)
}

#[test]
fn cap_zero_is_invalid() {
    let (r, _) = wide("[%s]", "hello", 0);
    assert_eq!(r, Err(FmtErr::Invalid));
}

#[test]
fn exact_fit_terminates() {
    let (r, out) = wide("[%s]", "hello", 8);
    assert_eq!(r, Ok(7));
    assert_eq!(&out[..7], &[91, 104, 101, 108, 108, 111, 93]);
    assert_eq!(out[7], 0);
}

#[test]
fn truncation_is_overflow_after_terminating() {
    for cap in 1..=6 {
        let (r, out) = wide("[%s]", "hello", cap);
        assert_eq!(r, Err(FmtErr::Overflow), "cap {cap}");
        assert_eq!(out[cap - 1], 0, "cap {cap} NUL");
    }
}

#[test]
fn wide_output_units() {
    let mut va = SliceVa::new();
    let mut out = vec![0xAAAAu32; 8];
    let opts = FmtOptions::default();
    let r = vswprintf_core(
        &mut out,
        4,
        &[b'%' as u32, b'l' as u32, b'c' as u32],
        &mut va,
        &opts,
        &FakeFloat,
    );
    assert_eq!(r, Ok(1));
    assert_eq!(out[0], 0);
    assert_eq!(out[1], 0);
}

#[test]
fn cap_above_int_max_overflows() {
    let mut va = SliceVa::new();
    let mut out = [0xAAAAu32; 4];
    let opts = FmtOptions::default();
    let cap = (i32::MAX as usize).saturating_add(2);
    let r = vswprintf_core(&mut out, cap, &[b'x' as u32], &mut va, &opts, &FakeFloat);
    assert_eq!(r, Err(FmtErr::Overflow));
    assert_eq!(out[0], 0);
}
