// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use crate::limits::{EAGAIN, EINVAL, ENOSYS, ENOTSUP, EOVERFLOW, ETIMEDOUT, SEM_VALUE_MAX};

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

#[test]
fn size_and_static_initialiser() {
    assert_eq!(core::mem::size_of::<Sem>(), 32);
    let sem = Sem::new();
    assert!(sem.val.iter().all(|word| word.load(Ordering::Relaxed) == 0));
    assert_eq!(sem.value(), 0);
}

#[test]
fn init_private_sets_the_value_and_flag() {
    let kernel = kernel();
    let mut sem = Sem::new();
    assert_eq!(init(&kernel, &mut sem, 0, 5), Ok(()));
    assert_eq!(sem.value(), 5);
    assert_eq!(sem.val[2].load(Ordering::Relaxed), 128);
}

#[test]
fn init_rejects_a_value_above_the_maximum() {
    let kernel = kernel();
    let mut sem = Sem::new();
    assert_eq!(
        init(&kernel, &mut sem, 0, SEM_VALUE_MAX as u32 + 1),
        Err(EINVAL)
    );
}

#[test]
fn init_rejects_a_bad_pshared() {
    let kernel = kernel();
    let mut sem = Sem::new();
    assert_eq!(init(&kernel, &mut sem, 2, 1), Err(EINVAL));
}

#[test]
fn init_shared_is_enotsup_without_pg4() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut sem = Sem::new();
    assert_eq!(init(&kernel, &mut sem, 1, 1), Err(ENOTSUP));
}

#[test]
fn post_and_trywait_round_trip() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 0).unwrap();
    assert_eq!(trywait(&sem), Err(EAGAIN));
    assert_eq!(post(&kernel, &sem), Ok(()));
    assert_eq!(trywait(&sem), Ok(()));
    assert_eq!(sem.value(), 0);
}

#[test]
fn post_at_the_maximum_is_eoverflow() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 1).unwrap();
    sem.val[0].store(SEM_VALUE_MAX as u32, Ordering::Release);
    assert_eq!(post(&kernel, &sem), Err(EOVERFLOW));
}

#[test]
fn getvalue_masks_the_waiter_flag() {
    let sem = Sem::new();
    sem.val[0].store(0x8000_0003, Ordering::Release);
    assert_eq!(getvalue(&sem), 3);
}

#[test]
fn wait_takes_an_available_value_without_the_kernel() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 1).unwrap();
    assert_eq!(wait(&kernel, &sem), Ok(()));
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn a_timedwait_past_deadline_times_out() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 0).unwrap();
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(timedwait(&kernel, &sem, Some(at)), Err(ETIMEDOUT));
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn a_timedwait_with_a_future_deadline_waits() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 0).unwrap();
    kernel.set_wait_result(ETIMEDOUT.0 as i32);
    let at = Timespec {
        tv_sec: 100,
        tv_nsec: 0,
    };
    assert_eq!(timedwait(&kernel, &sem, Some(at)), Err(ETIMEDOUT));
    assert_eq!(kernel.wait_calls(), 1);
}

#[test]
fn clockwait_is_the_same_core() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 1).unwrap();
    assert_eq!(clockwait(&kernel, &sem, None), Ok(()));
}

#[test]
fn sem_open_close_unlink_are_enosys() {
    assert_eq!(open(b"/x", 0), Err(ENOSYS));
    let sem = Sem::new();
    assert_eq!(close(&sem), Err(ENOSYS));
    assert_eq!(unlink(b"/x"), Err(ENOSYS));
}

#[test]
fn destroy_is_a_no_op() {
    let sem = Sem::new();
    assert_eq!(destroy(&sem), Ok(()));
}

#[test]
fn post_wakes_a_waiter_when_flagged() {
    let kernel = kernel();
    let mut sem = Sem::new();
    init(&kernel, &mut sem, 0, 0).unwrap();
    sem.val[0].store(0x8000_0000, Ordering::Release);
    sem.val[1].store(1, Ordering::Release);
    assert_eq!(post(&kernel, &sem), Ok(()));
    assert_eq!(kernel.wake_calls(), 1);
}
