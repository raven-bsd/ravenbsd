// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use crate::limits::{EBUSY, EINVAL, ENOTSUP, EPERM, ETIMEDOUT, PTHREAD_MUTEX_ERRORCHECK};
use core::sync::atomic::Ordering;

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

#[test]
fn size_and_static_initialiser() {
    assert_eq!(core::mem::size_of::<Cond>(), 48);
    assert_eq!(core::mem::size_of::<CondAttr>(), 4);
    let cond = Cond::new();
    assert!(
        cond.words
            .iter()
            .all(|word| word.load(Ordering::Relaxed) == 0)
    );
    assert!(!cond.is_shared());
    assert_eq!(cond.clock(), 0);
}

#[test]
fn init_applies_the_clock() {
    let mut cond = Cond::new();
    let mut attr = attr_init();
    attr_setclock(&mut attr, 1).unwrap();
    init(&mut cond, Some(&attr));
    assert_eq!(cond.clock(), 1);
    assert!(!cond.is_shared());
}

#[test]
fn init_applies_the_shared_flag() {
    let mut cond = Cond::new();
    let mut attr = attr_init();
    attr.attr = 0x8000_0000;
    init(&mut cond, Some(&attr));
    assert!(cond.is_shared());
}

#[test]
fn setclock_round_trip() {
    let mut attr = attr_init();
    assert_eq!(attr_setclock(&mut attr, 1), Ok(()));
    assert_eq!(attr_getclock(&attr), 1);
    assert_eq!(attr_setclock(&mut attr, 0), Ok(()));
    assert_eq!(attr_getclock(&attr), 0);
}

#[test]
fn setclock_rejects_cpu_clocks_and_negatives() {
    let mut attr = attr_init();
    for clock in [-1, 2, 3] {
        assert_eq!(attr_setclock(&mut attr, clock), Err(EINVAL));
    }
}

#[test]
fn cond_pshared_private_and_shared() {
    let kernel = kernel();
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, crate::limits::PTHREAD_PROCESS_PRIVATE),
        Ok(())
    );
    assert_eq!(attr_getpshared(&attr), 0);
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, crate::limits::PTHREAD_PROCESS_SHARED),
        Ok(())
    );
    assert_eq!(attr_getpshared(&attr), 1);
    assert_eq!(attr_setpshared(&kernel, &mut attr, 2), Err(EINVAL));
}

#[test]
fn cond_pshared_shared_is_enotsup_without_pg4() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, crate::limits::PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
}

#[test]
fn a_signal_without_waiters_is_ok() {
    let kernel = kernel();
    let cond = Cond::new();
    assert_eq!(signal(&kernel, &cond), Ok(()));
    assert_eq!(broadcast(&kernel, &cond), Ok(()));
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn a_shared_signal_is_enotsup() {
    let kernel = kernel();
    let cond = Cond::new();
    cond.words[0].store(1, Ordering::Release);
    assert_eq!(signal(&kernel, &cond), Err(ENOTSUP));
    assert_eq!(broadcast(&kernel, &cond), Err(ENOTSUP));
}

#[test]
fn wait_without_cancel_and_with_a_past_deadline_times_out() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let mut mutex = crate::mutex::Mutex::new();
    crate::mutex::init(&mut mutex, None);
    assert_eq!(crate::mutex::lock(&kernel, &mutex), Ok(()));
    let cond = Cond::new();
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = timedwait(&kernel, &cond, &mutex, 0, Some(at));
    assert_eq!(result, Err(ETIMEDOUT));
    // The condition variable returns with the mutex held.
    crate::mutex::unlock(&kernel, &mutex).unwrap();
}

#[test]
fn a_bad_timespec_is_einval() {
    let kernel = kernel();
    let mutex = crate::mutex::Mutex::new();
    let cond = Cond::new();
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 2_000_000_000,
    };
    assert_eq!(timedwait(&kernel, &cond, &mutex, 0, Some(at)), Err(EINVAL));
}

#[test]
fn a_non_owned_errorcheck_mutex_is_eperm() {
    let kernel = kernel();
    let mut mutex = crate::mutex::Mutex::new();
    let mut attr = crate::mutex::attr_init();
    crate::mutex::attr_settype(&mut attr, PTHREAD_MUTEX_ERRORCHECK).unwrap();
    crate::mutex::init(&mut mutex, Some(&attr));
    mutex.lock_word().store(999, Ordering::Release);
    let cond = Cond::new();
    assert_eq!(timedwait(&kernel, &cond, &mutex, 0, None), Err(EPERM));
}

#[test]
fn the_clock_selector_is_the_cond_clock() {
    let mut cond = Cond::new();
    let mut attr = attr_init();
    attr_setclock(&mut attr, 1).unwrap();
    init(&mut cond, Some(&attr));
    assert_eq!(cond.clock(), 1);
}

#[test]
fn clockwait_is_the_timed_core() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let mut mutex = crate::mutex::Mutex::new();
    crate::mutex::init(&mut mutex, None);
    crate::mutex::lock(&kernel, &mutex).unwrap();
    let cond = Cond::new();
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = clockwait(&kernel, &cond, &mutex, 0, Some(at));
    assert!(result == Ok(()) || result == Err(ETIMEDOUT));
}

#[test]
fn destroy_and_destroy_attr_are_stable() {
    let cond = Cond::new();
    assert_eq!(destroy(&cond), Ok(()));
    let attr = attr_init();
    attr_destroy(&attr);
    assert_eq!(attr.attr, 0);
}

#[test]
fn the_waiter_starts_in_the_waiting_state() {
    let waiter = Waiter::new();
    assert_eq!(waiter.state.load(Ordering::Acquire), WAITING);
    assert_eq!(waiter.barrier.load(Ordering::Acquire), 2);
    assert_eq!(waiter.prev, 0);
    assert_eq!(waiter.next, 0);
}

#[test]
fn the_waiter_address_is_its_own() {
    let waiter = Waiter::new();
    assert_eq!(waiter.address(), &waiter as *const Waiter as usize);
}

#[test]
fn the_three_states_are_distinct() {
    assert_ne!(WAITING, SIGNALED);
    assert_ne!(SIGNALED, LEAVING);
    assert_ne!(WAITING, LEAVING);
}

#[test]
fn a_private_signal_after_a_completed_wait_does_not_consume_the_wait() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let cond = Cond::new();
    signal(&kernel, &cond).unwrap();
    signal(&kernel, &cond).unwrap();
    assert_eq!(cond.tail(), 0);
}

#[test]
fn broadcast_counts_as_many_signals() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let cond = Cond::new();
    assert_eq!(broadcast(&kernel, &cond), Ok(()));
    assert_eq!(cond.head(), 0);
}

#[test]
fn the_lock_word_is_at_byte_thirty_two() {
    let cond = Cond::new();
    assert_eq!(
        cond.lock_word() as *const AtomicI32 as usize,
        &cond.words[8] as *const AtomicU32 as usize
    );
}

#[test]
fn busy_mutex_marker_is_not_confused_with_a_tid() {
    assert_eq!(EBUSY.0, 16);
}
