// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;

#[test]
fn getschedparam_returns_other_zero() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(getschedparam(&kernel, kernel.tcb()), Ok((SCHED_OTHER, 0)));
}

#[test]
fn getschedparam_of_a_dead_thread_is_esrch() {
    let kernel = crate::test_support::MockKernel::new();
    crate::thread::current_pthread(&kernel)
        .tid
        .store(0, core::sync::atomic::Ordering::Release);
    assert_eq!(getschedparam(&kernel, kernel.tcb()), Err(ESRCH));
}

#[test]
fn setschedparam_accepts_other_zero() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(setschedparam(&kernel, kernel.tcb(), SCHED_OTHER, 0), Ok(()));
}

#[test]
fn setschedparam_rejects_a_policy() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(
        setschedparam(&kernel, kernel.tcb(), crate::limits::SCHED_FIFO, 1),
        Err(EINVAL)
    );
}

#[test]
fn a_nonzero_priority_is_enotsup_without_pg11() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    assert_eq!(
        setschedparam(&kernel, kernel.tcb(), SCHED_OTHER, 1),
        Err(ENOTSUP)
    );
}

#[test]
fn setschedprio_goes_through_the_same_path() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(setschedprio(&kernel, kernel.tcb(), 0), Ok(()));
    assert_eq!(
        setschedprio(&kernel, crate::test_support::MockKernel::new().tcb(), 0),
        Ok(())
    );
}

#[test]
fn concurrency_get_set() {
    assert_eq!(getconcurrency(), 0);
    assert_eq!(setconcurrency(0), Ok(()));
    assert_eq!(setconcurrency(-1), Err(EINVAL));
    assert_eq!(setconcurrency(1), Err(crate::limits::EAGAIN));
}

#[test]
fn affinity_is_enosys_or_all_online() {
    assert_eq!(setaffinity(1, 1), Err(ENOSYS));
    assert_eq!(getaffinity(), Ok(1));
}
