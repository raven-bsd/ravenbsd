// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `nice` tests.

use core::cell::Cell;
use raven_abi::errno::Errno;

use crate::sched::nice::{NZERO, clamp_priority, nice};

#[test]
fn clamp_is_the_linux_range() {
    assert_eq!(clamp_priority(-100), -NZERO);
    assert_eq!(clamp_priority(0), 0);
    assert_eq!(clamp_priority(100), NZERO - 1);
}

#[test]
fn boundaries_are_inclusive() {
    assert_eq!(clamp_priority(-20), -20);
    assert_eq!(clamp_priority(19), 19);
}

#[test]
fn without_procd_negative_is_eperm() {
    assert_eq!(nice(-1, None, |_| Ok(())), Err(Errno::EPERM));
}

#[test]
fn without_procd_non_negative_is_a_no_op_success() {
    assert_eq!(nice(0, None, |_| panic!("must not set")), Ok(0));
    assert_eq!(nice(5, None, |_| panic!("must not set")), Ok(0));
}

#[test]
fn clamp_happens_before_setpriority() {
    let seen = Cell::new(0i32);
    let result = nice(100, Some(10), |value| {
        seen.set(value);
        Ok(())
    });
    assert_eq!(result, Ok(19));
    assert_eq!(seen.get(), 19, "procd sees the clamped value");
}

#[test]
fn set_errors_propagate() {
    assert_eq!(nice(1, Some(0), |_| Err(Errno::EACCES)), Err(Errno::EACCES));
}
