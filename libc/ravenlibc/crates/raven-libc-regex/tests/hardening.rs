// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The hardening limits and the ReDoS timings (ticket 7, release for timing).

mod common;

use std::time::{Duration, Instant};

use common::{CountingAllocator, EXHAUST_MAXSIZE, exhaust_shape, peak_bytes, reset_peak};
use raven_libc_regex::consts::{
    BACKREF_BUDGET, BACKREF_DEPTH_MAX, MAX_PAREN_DEPTH, MAX_STRIP, REG_ESPACE, REG_EXTENDED,
    REG_NOMATCH,
};
use raven_libc_regex::{ByteLocale, RegError, RegMatch, regcomp, regexec};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// A timing limit, ten times looser in a debug build (never skipped).
fn limit(seconds: u64) -> Duration {
    if cfg!(debug_assertions) {
        Duration::from_secs(seconds * 10)
    } else {
        Duration::from_secs(seconds)
    }
}

/// Run `nmatch = 0`.
fn matches(re: &raven_libc_regex::Regex, subject: &[u8]) -> Result<(), RegError> {
    let mut slots: [RegMatch; 0] = [];
    regexec(re, subject, &mut slots, 0, &ByteLocale)
}

#[test]
fn strip_cap_nested_repeat() {
    let _serial = common::serial();
    reset_peak();
    let started = Instant::now();
    let result = regcomp(b"((a{255}){255}){255}", REG_EXTENDED, &ByteLocale);
    let elapsed = started.elapsed();
    assert!(matches!(result, Err(RegError(REG_ESPACE))));
    assert!(elapsed < limit(1), "took {elapsed:?}");
    assert!(
        peak_bytes() < 64 * 1024 * 1024,
        "peak heap {} bytes",
        peak_bytes()
    );
    println!(
        "strip_cap_nested_repeat: {elapsed:?}, peak {} bytes",
        peak_bytes()
    );
}

/// Build a pattern whose strip is exactly `extra` sops over 8000 `a{255}` groups.
///
/// The groups stop below the cap so that the final `dupl` capacity check (`ssize + len`, the
/// donor's expression) stays inside [`MAX_STRIP`]; the literal padding grows the strip to the
/// exact cap through `doemit`.
fn boundary_pattern(extra: usize) -> Vec<u8> {
    const GROUPS: usize = 7000;
    const BASE: usize = 2 + 255 * GROUPS;
    let padding = MAX_STRIP.saturating_sub(BASE).saturating_add(extra).max(2);
    let mut pattern = Vec::new();
    for _ in 0..GROUPS {
        pattern.extend_from_slice(b"a{255}");
    }
    pattern.extend(std::iter::repeat_n(b'a', padding));
    pattern
}

#[test]
fn strip_cap_boundary() {
    let _serial = common::serial();
    reset_peak();
    let exact = boundary_pattern(0);
    let re = regcomp(&exact, REG_EXTENDED, &ByteLocale)
        .unwrap_or_else(|error| panic!("boundary regcomp {}", error.0));
    assert_eq!(re.nstates(), MAX_STRIP);
    println!(
        "strip_cap_boundary: nstates={} peak {} bytes",
        re.nstates(),
        peak_bytes()
    );
    drop(re);
    let one_more = boundary_pattern(1);
    assert!(matches!(
        regcomp(&one_more, REG_EXTENDED, &ByteLocale),
        Err(RegError(REG_ESPACE))
    ));
}

#[test]
fn paren_depth_boundary() {
    let _serial = common::serial();
    let mut pattern = vec![b'('; MAX_PAREN_DEPTH];
    pattern.push(b'a');
    pattern.extend(std::iter::repeat_n(b')', MAX_PAREN_DEPTH));
    let re = regcomp(&pattern, REG_EXTENDED, &ByteLocale)
        .unwrap_or_else(|error| panic!("paren depth regcomp {}", error.0));
    drop(re);

    pattern.insert(0, b'(');
    pattern.push(b')');
    assert!(matches!(
        regcomp(&pattern, REG_EXTENDED, &ByteLocale),
        Err(RegError(REG_ESPACE))
    ));

    // The BRE form: \( ... \).
    let mut bre = Vec::new();
    for _ in 0..(MAX_PAREN_DEPTH + 1) {
        bre.extend_from_slice(b"\\(");
    }
    bre.push(b'a');
    for _ in 0..(MAX_PAREN_DEPTH + 1) {
        bre.extend_from_slice(b"\\)");
    }
    assert!(matches!(
        regcomp(&bre, 0, &ByteLocale),
        Err(RegError(REG_ESPACE))
    ));
    println!("paren_depth_boundary: 256 compile, 257 REG_ESPACE");
}

#[test]
fn backref_budget_nested_star() {
    let _serial = common::serial();
    let re = regcomp(b"\\(a*\\)*\\1", 0, &ByteLocale)
        .unwrap_or_else(|error| panic!("nested star regcomp {}", error.0));
    let mut subject = vec![b'a'; 30];
    subject.push(b'b');
    let started = Instant::now();
    let result = matches(&re, &subject);
    let elapsed = started.elapsed();
    let code = match result {
        Ok(()) => 0,
        Err(error) => error.0,
    };
    assert!(elapsed < limit(5), "took {elapsed:?}");
    assert!(
        code == 0 || code == REG_ESPACE || code == REG_NOMATCH,
        "unexpected {code}"
    );
    println!("backref_budget_nested_star: 30 a + b -> {elapsed:?}, error {code}");

    let mut ten = vec![b'a'; 10];
    let started = Instant::now();
    let _ = matches(&re, &ten);
    let elapsed = started.elapsed();
    assert!(elapsed < limit(5), "took {elapsed:?}");
    ten.push(b'b');
    let started = Instant::now();
    let _ = matches(&re, &ten);
    assert!(started.elapsed() < limit(5));
}

#[test]
fn backref_budget_long_group() {
    let _serial = common::serial();
    let re = regcomp(b"\\(.*\\)\\1", 0, &ByteLocale)
        .unwrap_or_else(|error| panic!("long group regcomp {}", error.0));
    let subject = vec![b'a'; 100_000];
    let started = Instant::now();
    let result = matches(&re, &subject);
    let elapsed = started.elapsed();
    let code = match result {
        Ok(()) => 0,
        Err(error) => error.0,
    };
    assert!(elapsed < limit(5), "took {elapsed:?}");
    assert!(code == 0 || code == REG_ESPACE || code == REG_NOMATCH);
    println!("backref_budget_long_group: 100000 bytes -> {elapsed:?}, error {code}");
}

#[test]
fn backref_budget_constant() {
    let _serial = common::serial();
    assert_eq!(BACKREF_BUDGET, 1 << 24);
    assert_eq!(BACKREF_DEPTH_MAX, 4096);
}

#[test]
fn linear_paths_unbudgeted() {
    let _serial = common::serial();
    let subject = vec![b'a'; 10_000];
    for pattern in [&b"(a*)*b"[..], &b"(a|aa)+$"[..]] {
        let re = regcomp(pattern, REG_EXTENDED, &ByteLocale)
            .unwrap_or_else(|error| panic!("linear regcomp {}", error.0));
        let started = Instant::now();
        let _ = matches(&re, &subject);
        let elapsed = started.elapsed();
        assert!(elapsed < limit(2), "pattern {pattern:?} took {elapsed:?}");
        println!(
            "linear_paths_unbudgeted: {} -> {elapsed:?}",
            String::from_utf8_lossy(pattern)
        );
    }
}

#[test]
fn exhaust_seven_shapes_at_maxsize() {
    let _serial = common::serial();
    reset_peak();
    let started = Instant::now();
    for index in 0..7 {
        let pattern = exhaust_shape(index, EXHAUST_MAXSIZE);
        match regcomp(&pattern, common::EXHAUST_FLAGS[index], &ByteLocale) {
            Ok(re) => {
                let _ = matches(&re, b"aaaaaaaaaaa");
                drop(re);
            }
            Err(RegError(code)) => assert_eq!(code, REG_ESPACE, "shape {index}"),
        }
    }
    let elapsed = started.elapsed();
    assert!(peak_bytes() < 256 * 1024 * 1024, "peak {}", peak_bytes());
    println!(
        "exhaust_seven_shapes_at_maxsize: {elapsed:?}, peak {} bytes",
        peak_bytes()
    );
}

#[test]
fn backref_depth_small_stack() {
    let _serial = common::serial();
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let re = regcomp(b"\\(a*\\)*\\1", 0, &ByteLocale)
                .unwrap_or_else(|error| panic!("small stack regcomp {}", error.0));
            let mut subject = vec![b'a'; 10_000];
            subject.push(b'b');
            matches(&re, &subject)
        })
        .expect("spawn");
    let result = handle.join().expect("no stack overflow and no panic");
    assert!(matches!(result, Err(RegError(REG_ESPACE))));
    println!("backref_depth_small_stack: REG_ESPACE in a 256 KiB thread");
}

#[test]
fn the_limit_constants_are_the_ticket_values() {
    let _serial = common::serial();
    assert_eq!(MAX_STRIP, 1 << 21);
    assert_eq!(MAX_PAREN_DEPTH, 256);
    assert_eq!(BACKREF_DEPTH_MAX, 4096);
}
