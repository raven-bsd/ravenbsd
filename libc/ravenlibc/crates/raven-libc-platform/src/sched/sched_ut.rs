// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheduler module tests.

#[test]
fn nzero_is_20() {
    assert_eq!(crate::sched::NZERO, 20);
}

#[test]
fn clamp_uses_nzero() {
    assert_eq!(crate::sched::clamp_priority(-1000), -20);
}
