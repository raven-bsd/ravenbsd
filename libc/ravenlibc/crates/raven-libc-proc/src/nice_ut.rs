// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `nice`.

use super::*;

struct Fake {
    old: i32,
    get_calls: usize,
    set_calls: usize,
    set_value: i32,
    get_error: Option<Errno>,
    set_error: Option<Errno>,
}

impl Fake {
    fn new(old: i32) -> Self {
        Self {
            old,
            get_calls: 0,
            set_calls: 0,
            set_value: 0,
            get_error: None,
            set_error: None,
        }
    }
}

impl PrioOps for Fake {
    fn getpriority(&mut self, _which: u32, _who: i32) -> Result<i32, Errno> {
        self.get_calls += 1;
        match self.get_error {
            Some(error) => Err(error),
            None => Ok(self.old),
        }
    }

    fn setpriority(&mut self, _which: u32, _who: i32, prio: i32) -> Result<(), Errno> {
        self.set_calls += 1;
        self.set_value = prio;
        match self.set_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[test]
fn nice_zero_returns_the_old_priority() {
    let mut ops = Fake::new(5);
    assert_eq!(nice(&mut ops, 0), Ok(5));
    assert_eq!(ops.get_calls, 1);
    assert_eq!(ops.set_value, 5);
}

#[test]
fn a_positive_increment_is_added() {
    let mut ops = Fake::new(5);
    assert_eq!(nice(&mut ops, 10), Ok(15));
    assert_eq!(ops.set_value, 15);
}

#[test]
fn the_upper_bound_is_clamped() {
    let mut ops = Fake::new(5);
    assert_eq!(nice(&mut ops, 40), Ok(19));
    assert_eq!(ops.get_calls, 0);
    assert_eq!(ops.set_value, 19);
}

#[test]
fn the_lower_bound_is_clamped() {
    let mut ops = Fake::new(5);
    assert_eq!(nice(&mut ops, -40), Ok(-20));
    assert_eq!(ops.get_calls, 0);
    assert_eq!(ops.set_value, -20);
}

#[test]
fn a_negative_increment_is_added_before_clamping() {
    let mut ops = Fake::new(5);
    assert_eq!(nice(&mut ops, -20), Ok(-15));
    assert_eq!(ops.set_value, -15);
}

#[test]
fn eacces_becomes_eperm() {
    let mut ops = Fake::new(0);
    ops.set_error = Some(EACCES);
    assert_eq!(nice(&mut ops, 0), Err(EPERM));
}

#[test]
fn other_set_errors_pass_through() {
    let mut ops = Fake::new(0);
    ops.set_error = Some(crate::errors::EINVAL);
    assert_eq!(nice(&mut ops, 0), Err(crate::errors::EINVAL));
}

#[test]
fn getpriority_errors_pass_through() {
    let mut ops = Fake::new(0);
    ops.get_error = Some(crate::errors::ESRCH);
    assert_eq!(nice(&mut ops, 0), Err(crate::errors::ESRCH));
    assert_eq!(ops.set_calls, 0);
}
