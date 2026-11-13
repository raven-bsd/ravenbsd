// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::{FakeFloat, SliceVa};

fn core(fmt: &str, text: &str, cap: usize) -> (Result<i32, FmtErr>, Vec<u8>) {
    let mut va = SliceVa::new();
    let p = va.cstr(text);
    va.ptr(p);
    let mut out = vec![0xAAu8; cap.saturating_add(4)];
    let opts = FmtOptions::default();
    let r = vsnprintf_core(&mut out, cap, fmt.as_bytes(), &mut va, &opts, &FakeFloat);
    (r, out)
}

#[test]
fn cap_zero_returns_the_length_and_writes_nothing() {
    let (r, out) = core("[%s]", "hello", 0);
    assert_eq!(r, Ok(7));
    assert!(out.iter().all(|&b| b == 0xAA));
}

#[test]
fn cap_one_writes_only_the_nul() {
    let (r, out) = core("[%s]", "hello", 1);
    assert_eq!(r, Ok(7));
    assert_eq!(out[0], 0);
    assert_eq!(out[1], 0xAA);
}

#[test]
fn truncation_matrix() {
    let text = "hello";
    let full = "[hello]";
    for cap in 0..=full.len() + 2 {
        let (r, out) = core("[%s]", text, cap);
        assert_eq!(r, Ok(full.len() as i32), "cap {cap}");
        if cap == 0 {
            assert!(out.iter().all(|&b| b == 0xAA), "cap 0 must not write");
            continue;
        }
        let stored = std::cmp::min(cap - 1, full.len());
        assert_eq!(&out[..stored], &full.as_bytes()[..stored], "cap {cap}");
        assert_eq!(out[stored], 0, "cap {cap} NUL");
        if cap <= full.len() {
            assert_eq!(out[stored + 1], 0xAA, "cap {cap} guard");
        }
    }
}

#[test]
fn cap_minus_one_above_int_max_overflows() {
    let mut va = SliceVa::new();
    let mut out = [0xAAu8; 8];
    let opts = FmtOptions::default();
    let cap = (i32::MAX as usize).saturating_add(2);
    let r = vsnprintf_core(&mut out, cap, b"x", &mut va, &opts, &FakeFloat);
    assert_eq!(r, Err(FmtErr::Overflow));
    assert_eq!(out[0], 0);
    assert_eq!(out[1], 0xAA);
}

#[test]
fn errors_still_terminate() {
    let mut va = SliceVa::new();
    va.dbl(1.5);
    let mut out = [0xAAu8; 8];
    let opts = FmtOptions::default();
    let r = vsnprintf_core(&mut out, 8, b"%f", &mut va, &opts, &crate::NoFloat);
    assert_eq!(r, Err(FmtErr::Invalid));
    assert_eq!(out[0], 0);
}
