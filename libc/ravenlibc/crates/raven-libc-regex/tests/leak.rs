// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The allocator-baseline leak check of the hardening error path.
//!
//! This lives in its own single-test binary: the counting allocator is process-global, and a
//! parallel test harness would otherwise perturb the live-byte baseline. The compile/`REG_ESPACE`
//! boundary itself is in `tests/hardening.rs`.

mod common;

use common::{CountingAllocator, live_bytes};
use raven_libc_regex::consts::{MAX_PAREN_DEPTH, REG_ESPACE, REG_EXTENDED};
use raven_libc_regex::{ByteLocale, RegError, regcomp};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn too_deep(bre: bool) -> Vec<u8> {
    let mut pattern = Vec::new();
    if bre {
        for _ in 0..(MAX_PAREN_DEPTH + 1) {
            pattern.extend_from_slice(b"\\(");
        }
        pattern.push(b'a');
        for _ in 0..(MAX_PAREN_DEPTH + 1) {
            pattern.extend_from_slice(b"\\)");
        }
    } else {
        pattern.extend(std::iter::repeat_n(b'(', MAX_PAREN_DEPTH + 1));
        pattern.push(b'a');
        pattern.extend(std::iter::repeat_n(b')', MAX_PAREN_DEPTH + 1));
    }
    pattern
}

#[test]
fn paren_depth_boundary_no_leak() {
    for bre in [false, true] {
        let pattern = too_deep(bre);
        let baseline = live_bytes();
        let cflags = if bre { 0 } else { REG_EXTENDED };
        let result = regcomp(&pattern, cflags, &ByteLocale);
        assert!(matches!(result, Err(RegError(REG_ESPACE))));
        assert_eq!(live_bytes(), baseline, "the error path leaked (bre={bre})");
    }
}
