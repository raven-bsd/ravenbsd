// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::limits::{EINVAL, ENOTSUP, PTHREAD_STACK_MIN};

#[test]
fn size_and_alignment() {
    assert_eq!(core::mem::size_of::<Attr>(), 56);
    assert_eq!(core::mem::align_of::<Attr>(), 8);
}

#[test]
fn init_fills_the_process_defaults() {
    let attr = init();
    assert_eq!(attr.stacksize(), LIBC.default_stack());
    assert_eq!(attr.guardsize(), LIBC.default_guard());
    assert_eq!(attr.detach_state(), PTHREAD_CREATE_JOINABLE);
}

#[test]
fn destroy_is_a_no_op() {
    let attr = init();
    destroy(&attr);
    assert_eq!(attr.detach_state(), 0);
}

#[test]
fn detach_state_round_trip() {
    let mut attr = init();
    assert_eq!(setdetachstate(&mut attr, PTHREAD_CREATE_DETACHED), Ok(()));
    assert_eq!(getdetachstate(&attr), PTHREAD_CREATE_DETACHED);
    assert_eq!(setdetachstate(&mut attr, PTHREAD_CREATE_JOINABLE), Ok(()));
    assert_eq!(getdetachstate(&attr), PTHREAD_CREATE_JOINABLE);
}

#[test]
fn detach_state_rejects_a_bad_value() {
    let mut attr = init();
    for state in [-1, 2, 100] {
        assert_eq!(setdetachstate(&mut attr, state), Err(EINVAL));
    }
}

#[test]
fn guardsize_round_trip() {
    let mut attr = init();
    assert_eq!(setguardsize(&mut attr, 8192), Ok(()));
    assert_eq!(getguardsize(&attr), 8192);
    assert_eq!(setguardsize(&mut attr, 0), Ok(()));
    assert_eq!(getguardsize(&attr), 0);
}

#[test]
fn guardsize_rejects_an_overflowing_value() {
    let mut attr = init();
    assert_eq!(setguardsize(&mut attr, usize::MAX), Err(EINVAL));
}

#[test]
fn stacksize_round_trip() {
    let mut attr = init();
    assert_eq!(setstacksize(&mut attr, 65536), Ok(()));
    assert_eq!(getstacksize(&attr), 65536);
}

#[test]
fn stacksize_below_the_minimum_is_einval() {
    let mut attr = init();
    assert_eq!(setstacksize(&mut attr, PTHREAD_STACK_MIN - 1), Err(EINVAL));
    assert_eq!(setstacksize(&mut attr, 0), Err(EINVAL));
}

#[test]
fn stacksize_at_the_minimum_is_accepted() {
    let mut attr = init();
    assert_eq!(setstacksize(&mut attr, PTHREAD_STACK_MIN), Ok(()));
}

#[test]
fn setstack_round_trip() {
    let mut attr = init();
    assert_eq!(setstack(&mut attr, 0x1000_0000, 65536), Ok(()));
    assert_eq!(getstack(&attr), Ok((0x1000_0000, 65536)));
    assert_eq!(attr.stackaddr(), 0x1000_0000 + 65536);
}

#[test]
fn getstack_without_a_user_stack_is_einval() {
    let attr = init();
    assert_eq!(getstack(&attr), Err(EINVAL));
}

#[test]
fn setstack_below_the_minimum_is_einval() {
    let mut attr = init();
    assert_eq!(
        setstack(&mut attr, 0x1000, PTHREAD_STACK_MIN - 1),
        Err(EINVAL)
    );
}

#[test]
fn inheritsched_round_trip() {
    let mut attr = init();
    assert_eq!(setinheritsched(&mut attr, PTHREAD_EXPLICIT_SCHED), Ok(()));
    assert_eq!(getinheritsched(&attr), PTHREAD_EXPLICIT_SCHED);
    assert_eq!(setinheritsched(&mut attr, PTHREAD_INHERIT_SCHED), Ok(()));
    assert_eq!(getinheritsched(&attr), PTHREAD_INHERIT_SCHED);
    assert_eq!(setinheritsched(&mut attr, 2), Err(EINVAL));
}

#[test]
fn schedparam_round_trip() {
    let mut attr = init();
    setschedparam(&mut attr, 7);
    assert_eq!(getschedparam(&attr), 7);
}

#[test]
fn policy_other_is_accepted() {
    let kernel = crate::test_support::MockKernel::new();
    let mut attr = init();
    assert_eq!(setschedpolicy(&kernel, &mut attr, SCHED_OTHER), Ok(()));
    assert_eq!(getschedpolicy(&attr), SCHED_OTHER);
}

#[test]
fn policy_fifo_is_enotsup_without_pg11() {
    let kernel = crate::test_support::MockKernel::with_caps(crate::kernel::KernelCaps::minimal());
    let mut attr = init();
    assert_eq!(
        setschedpolicy(&kernel, &mut attr, crate::limits::SCHED_FIFO),
        Err(ENOTSUP)
    );
}

#[test]
fn policy_fifo_is_accepted_with_pg11() {
    let kernel = crate::test_support::MockKernel::with_caps(crate::kernel::KernelCaps::full());
    let mut attr = init();
    assert_eq!(
        setschedpolicy(&kernel, &mut attr, crate::limits::SCHED_FIFO),
        Ok(())
    );
    assert_eq!(getschedpolicy(&attr), crate::limits::SCHED_FIFO);
}

#[test]
fn scope_system_is_accepted() {
    let mut attr = init();
    assert_eq!(setscope(&mut attr, PTHREAD_SCOPE_SYSTEM), Ok(()));
    assert_eq!(getscope(&attr), PTHREAD_SCOPE_SYSTEM);
    assert_eq!(setscope(&mut attr, PTHREAD_SCOPE_PROCESS), Err(ENOTSUP));
    assert_eq!(setscope(&mut attr, 99), Err(EINVAL));
}

#[test]
fn default_np_rejects_extra_fields() {
    let _serial = crate::test_support::serial();
    let mut attr = init();
    attr.words[6] = 1;
    assert_eq!(setattr_default_np(&attr), Err(EINVAL));
}

#[test]
fn default_np_accepts_stack_and_guard() {
    let _serial = crate::test_support::serial();
    let mut attr = init();
    attr.set_stacksize_raw(65536);
    raw::store_usize(&mut attr.words, 2, 8192);
    assert_eq!(setattr_default_np(&attr), Ok(()));
}

#[test]
fn getattr_default_np_returns_the_process_defaults() {
    let _serial = crate::test_support::serial();
    let attr = getattr_default_np();
    assert_eq!(attr.stacksize(), LIBC.default_stack());
    assert_eq!(attr.guardsize(), LIBC.default_guard());
}

#[test]
fn getattr_np_reports_the_given_tcb() {
    let kernel = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread_mut(&kernel);
    pthread.stack_base = 0x1000_0000;
    pthread.stack_size = 0x10000;
    pthread.guard_size = 4096;
    let attr = getattr_np(pthread);
    assert_eq!(attr.stacksize(), 0x10000);
    assert_eq!(attr.guardsize(), 4096);
    assert_eq!(attr.stackaddr(), 0x1001_0000);
    assert_eq!(attr.detach_state(), PTHREAD_CREATE_JOINABLE);
}

#[test]
fn getattr_np_main_thread_falls_back_to_eight_mib() {
    let kernel = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&kernel);
    let attr = getattr_np(pthread);
    assert_eq!(attr.stacksize(), crate::limits::DEFAULT_STACK_SIZE);
}
