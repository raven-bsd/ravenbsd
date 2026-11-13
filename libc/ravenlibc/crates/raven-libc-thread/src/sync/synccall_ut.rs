// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;

#[test]
fn without_kick_the_callback_runs_on_the_caller() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut ran = 0;
    let kicked = synccall(&kernel, || ran += 1);
    assert_eq!(kicked, 0);
    assert_eq!(ran, 1);
    assert_eq!(kernel.kick_calls(), 0);
}

#[test]
fn with_kick_every_live_thread_is_kicked() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    crate::libc_state::LIBC.reset(kernel.tcb());
    crate::thread::current_pthread_mut(&kernel).handle = 9;
    let kicked = synccall(&kernel, || {});
    assert_eq!(kicked, 1);
    assert_eq!(kernel.kick_calls(), 1);
}

#[test]
fn the_signal_bit_matches_sigsynccall() {
    assert_eq!(SIGSYNCCALL_BIT, 1 << 33);
}

#[test]
fn a_callback_without_an_initialised_list_still_runs() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    crate::libc_state::LIBC.reset(0);
    let mut ran = false;
    let kicked = synccall(&kernel, || ran = true);
    assert_eq!(kicked, 0);
    assert!(ran);
}
