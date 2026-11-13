// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The seven exhaust shapes of `t_exhaust.c`, rewritten from behaviour.
//!
//! The donor test (BSD-4, STUDY only) builds nested groups, `(.*)`, `((.*){0,255})`,
//! `(.\\{0,})`, `((.*){1,255})`, `(...){1,100}` and `(?:(.*)|(.*))` forms up to
//! `REGEX_MAXSIZE = 9999` and requires that `regcomp` returns 0 or exactly `REG_ESPACE` and
//! that `regexec` of `aaaaaaaaaaa` does not crash. The counting allocator keeps the peak
//! heap under 256 MiB (the donor's `require.memory`).

mod common;

use common::{
    CountingAllocator, EXHAUST_FLAGS, EXHAUST_MAXSIZE, exhaust_shape, peak_bytes, reset_peak,
};
use raven_libc_regex::consts::{REG_ESPACE, REG_NOMATCH};
use raven_libc_regex::{ByteLocale, RegError, RegMatch, regcomp, regexec};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// `regcomp` must return 0 or `REG_ESPACE`; a success must survive one `regexec`.
fn one_case(index: usize, len: usize) -> (bool, usize) {
    reset_peak();
    let pattern = exhaust_shape(index, len);
    match regcomp(&pattern, EXHAUST_FLAGS[index], &ByteLocale) {
        Ok(re) => {
            let mut slots: [RegMatch; 0] = [];
            let result = regexec(&re, b"aaaaaaaaaaa", &mut slots, 0, &ByteLocale);
            assert!(
                matches!(result, Ok(()) | Err(RegError(REG_NOMATCH))),
                "shape {index} len {len}: regexec failed with {}",
                result.err().map_or(0, |error| error.0)
            );
            (true, peak_bytes())
        }
        Err(RegError(code)) => {
            assert_eq!(code, REG_ESPACE, "shape {index} len {len}");
            (false, peak_bytes())
        }
    }
}

#[test]
fn seven_shapes_at_maxsize() {
    let _serial = common::serial();
    let mut compiled = 0usize;
    let mut errors = 0usize;
    let mut peak = 0usize;
    for index in 0..7 {
        let (ok, used) = one_case(index, EXHAUST_MAXSIZE);
        if ok {
            compiled += 1;
        } else {
            errors += 1;
        }
        peak = peak.max(used);
        assert!(used < 256 * 1024 * 1024, "shape {index}: peak {used}");
    }
    println!(
        "exhaust at {EXHAUST_MAXSIZE}: {compiled} compiled, {errors} REG_ESPACE, peak {peak} bytes"
    );
}

#[test]
fn seven_shapes_at_every_length() {
    let _serial = common::serial();
    let mut lengths = vec![1usize];
    while let Some(&last) = lengths.last() {
        let next = last * 2;
        if next > 8192 {
            break;
        }
        lengths.push(next);
    }
    let mut peak = 0usize;
    let mut cases = 0usize;
    for index in 0..7 {
        for &len in &lengths {
            let (_, used) = one_case(index, len);
            peak = peak.max(used);
            cases += 1;
        }
    }
    println!("exhaust lengths {lengths:?}: {cases} cases, peak {peak} bytes");
    assert!(peak < 256 * 1024 * 1024, "peak {peak}");
}
