// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};

static RUNS: AtomicUsize = AtomicUsize::new(0);

fn counting_init() {
    RUNS.fetch_add(1, Ordering::AcqRel);
}

#[test]
fn size_and_initial_value() {
    assert_eq!(core::mem::size_of::<Once>(), 4);
    let once = Once::new();
    assert_eq!(once.word.load(Ordering::Acquire), 0);
}

#[test]
fn the_init_runs_exactly_once() {
    let _serial = crate::test_support::serial();
    RUNS.store(0, Ordering::Release);
    let kernel = crate::test_support::MockKernel::new();
    let control = Once::new();
    once(&kernel, &control, counting_init);
    once(&kernel, &control, counting_init);
    once(&kernel, &control, counting_init);
    assert_eq!(RUNS.load(Ordering::Acquire), 1);
    assert_eq!(control.word.load(Ordering::Acquire), 2);
}

#[test]
fn the_fast_path_sees_state_two() {
    let kernel = crate::test_support::MockKernel::new();
    let control = Once::new();
    control.word.store(2, Ordering::Release);
    let points = kernel.points();
    once(&kernel, &control, counting_init);
    assert_eq!(kernel.points(), points);
}

#[test]
fn call_once_is_the_same_entry() {
    let _serial = crate::test_support::serial();
    RUNS.store(0, Ordering::Release);
    let kernel = crate::test_support::MockKernel::new();
    let control = Once::new();
    call_once(&kernel, &control, counting_init);
    assert_eq!(RUNS.load(Ordering::Acquire), 1);
    assert_eq!(control.word.load(Ordering::Acquire), 2);
}

#[test]
fn an_already_finished_control_is_not_rerun() {
    let _serial = crate::test_support::serial();
    RUNS.store(0, Ordering::Release);
    let kernel = crate::test_support::MockKernel::new();
    let control = Once::new();
    control.word.store(2, Ordering::Release);
    once(&kernel, &control, counting_init);
    assert_eq!(RUNS.load(Ordering::Acquire), 0);
}
