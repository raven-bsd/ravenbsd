// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `abort_sequence` records the donor's exact signal order.

use super::*;

#[derive(Default)]
struct Fake {
    calls: Vec<&'static str>,
}

impl AbortOps for Fake {
    fn unblock_sigabrt(&mut self) {
        self.calls.push("unblock");
    }
    fn raise_sigabrt(&mut self) {
        self.calls.push("raise");
    }
    fn reset_sigabrt_default(&mut self) {
        self.calls.push("default");
    }
    fn terminate(&mut self) {
        self.calls.push("terminate");
    }
}

#[test]
fn exact_sequence() {
    let mut fake = Fake::default();
    abort_sequence(&mut fake);
    assert_eq!(
        fake.calls,
        vec![
            "unblock",
            "raise",
            "default",
            "unblock",
            "raise",
            "terminate"
        ]
    );
}

#[test]
fn second_abort_call_repeats_the_sequence() {
    let mut fake = Fake::default();
    abort_sequence(&mut fake);
    abort_sequence(&mut fake);
    assert_eq!(fake.calls.len(), 12);
    assert_eq!(fake.calls.last(), Some(&"terminate"));
}
