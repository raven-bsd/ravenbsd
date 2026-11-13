// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The 138 FreeBSD `fnmatch_testcases.h` vectors.
//!
//! The table is generated data (`tests/data/freebsd_fnmatch.rs`). The musl donor is the
//! implementation; a vector whose expectation is FreeBSD-specific is listed as a declared
//! difference instead of being hidden.

use raven_libc_glob::{Utf8Locale, fnmatch};

include!("data/freebsd_fnmatch.rs");

const FNM_NOMATCH: i32 = 1;

fn m(pattern: &[u8], string: &[u8], flags: u32) -> i32 {
    fnmatch(pattern, string, flags, &Utf8Locale)
}

/// FreeBSD expectations that the musl donor does not share (reported in the C46 report).
fn known_difference(pattern: &[u8], string: &[u8], flags: u32) -> bool {
    // A lone trailing backslash: FreeBSD treats it as an escape error (no match), musl as an
    // ordinary character (fnmatch.c:57).
    matches!(
        (pattern, string, flags),
        (b"\\", b"\\", 0) | (b"\\", b"", 0)
    )
}

#[test]
fn freebsd_fnmatch_table() {
    let mut failures = Vec::new();
    let mut compared = 0;
    for &(pattern, string, flags, expected) in VECTORS {
        if known_difference(pattern, string, flags) {
            continue;
        }
        compared += 1;
        let got = m(pattern, string, flags);
        if got != expected {
            failures.push(format!(
                "pattern {:?} string {:?} flags {flags:#x}: want {expected} got {got}",
                String::from_utf8_lossy(pattern),
                String::from_utf8_lossy(string)
            ));
        }
    }
    assert_eq!(VECTORS.len(), 138);
    assert_eq!(compared, 136);
    assert!(
        failures.is_empty(),
        "{} of {compared} FreeBSD vectors failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_two_known_differences_are_the_trailing_backslash() {
    assert_eq!(m(b"\\", b"\\", 0), 0);
    assert_eq!(m(b"\\", b"", 0), FNM_NOMATCH);
}
