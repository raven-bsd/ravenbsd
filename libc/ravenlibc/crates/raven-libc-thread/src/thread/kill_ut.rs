// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use core::sync::atomic::Ordering;

#[test]
fn a_null_thread_is_esrch() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(kill(&kernel, 0, 1), Err(ESRCH));
}

#[test]
fn a_dead_thread_is_esrch() {
    let kernel = crate::test_support::MockKernel::new();
    crate::thread::current_pthread(&kernel)
        .tid
        .store(-1, Ordering::Release);
    assert_eq!(kill(&kernel, kernel.tcb(), 1), Err(ESRCH));
}

#[test]
fn signal_zero_on_a_live_thread_is_ok() {
    let kernel = crate::test_support::MockKernel::new();
    crate::thread::current_pthread_mut(&kernel).handle = 3;
    assert_eq!(kill(&kernel, kernel.tcb(), 0), Ok(()));
}

#[test]
fn a_signal_without_pg7_is_enosys() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    crate::thread::current_pthread_mut(&kernel).handle = 3;
    assert_eq!(kill(&kernel, kernel.tcb(), 1), Err(ENOSYS));
}

#[test]
fn a_bad_signal_is_einval() {
    let kernel = crate::test_support::MockKernel::new();
    crate::thread::current_pthread_mut(&kernel).handle = 3;
    assert_eq!(kill(&kernel, kernel.tcb(), NSIG), Err(EINVAL));
    assert_eq!(kill(&kernel, kernel.tcb(), -1), Err(EINVAL));
}

#[test]
fn a_kick_carries_the_signal_bit() {
    let kernel = crate::test_support::MockKernel::new();
    crate::thread::current_pthread_mut(&kernel).handle = 7;
    assert_eq!(kill(&kernel, kernel.tcb(), 9), Ok(()));
    assert_eq!(kernel.kick_calls(), 1);
}

#[test]
fn sigqueue_is_enosys() {
    assert_eq!(sigqueue(1, 9, 0), Err(ENOSYS));
}
