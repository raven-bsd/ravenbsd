// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deadline tests.

use super::*;
use core::cell::Cell;

struct Env(Cell<u64>);

impl NetEnv for Env {
    fn monotonic_ms(&self) -> u64 {
        self.0.get()
    }
    fn entropy(&self, _buf: &mut [u8]) -> bool {
        true
    }
    fn now_secs(&self) -> i64 {
        0
    }
}

#[test]
fn infinite_deadline_never_expires() {
    let env = Env(Cell::new(10));
    let deadline = Deadline::new(&env, None);
    assert_eq!(deadline.remaining(&env), None);
    assert!(!deadline.expired(&env));
    env.0.set(1_000_000);
    assert!(!deadline.expired(&env));
}

#[test]
fn finite_deadline_counts_down() {
    let env = Env(Cell::new(10));
    let deadline = Deadline::new(&env, Some(50));
    assert_eq!(deadline.end(), Some(60));
    assert_eq!(deadline.remaining(&env), Some(50));
    env.0.set(30);
    assert_eq!(deadline.remaining(&env), Some(30));
    env.0.set(60);
    assert_eq!(deadline.remaining(&env), Some(0));
    assert!(deadline.expired(&env));
    env.0.set(61);
    assert_eq!(deadline.remaining(&env), Some(0));
}
