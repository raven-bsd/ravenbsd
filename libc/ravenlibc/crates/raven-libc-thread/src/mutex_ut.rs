// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use crate::limits::{
    EBUSY, EDEADLK, ENOTSUP, EOWNERDEAD, EPERM, ETIMEDOUT, MUTEX_UNRECOVERABLE, OWNER_DIED,
};
use core::sync::atomic::Ordering;

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

fn recursive_attr() -> MutexAttr {
    let mut attr = attr_init();
    attr_settype(&mut attr, PTHREAD_MUTEX_RECURSIVE).unwrap();
    attr
}

fn errorcheck_attr() -> MutexAttr {
    let mut attr = attr_init();
    attr_settype(&mut attr, PTHREAD_MUTEX_ERRORCHECK).unwrap();
    attr
}

fn robust_attr() -> MutexAttr {
    let mut attr = attr_init();
    attr_setrobust(&mut attr, 1).unwrap();
    attr
}

#[test]
fn size_and_alignment_are_the_c_object() {
    assert_eq!(core::mem::size_of::<Mutex>(), 40);
    assert_eq!(core::mem::align_of::<Mutex>(), 8);
    assert_eq!(core::mem::size_of::<MutexAttr>(), 4);
}

#[test]
fn a_zeroed_mutex_is_the_static_initialiser() {
    let mutex = Mutex::new();
    assert!(
        mutex
            .words
            .iter()
            .all(|word| word.load(Ordering::Relaxed) == 0)
    );
    assert_eq!(mutex.type_word(), 0);
    assert_eq!(mutex.lock_value(), 0);
    assert!(!mutex.is_robust());
    assert!(!mutex.is_shared());
}

#[test]
fn init_clears_and_applies_the_type() {
    let mut mutex = Mutex::new();
    mutex.words[1].store(7, Ordering::Release);
    let attr = recursive_attr();
    init(&mut mutex, Some(&attr));
    assert_eq!(mutex.type_word(), PTHREAD_MUTEX_RECURSIVE);
    assert_eq!(mutex.lock_value(), 0);
}

#[test]
fn init_without_attr_is_normal() {
    let mut mutex = Mutex::new();
    init(&mut mutex, None);
    assert_eq!(mutex.kind(), PTHREAD_MUTEX_NORMAL);
}

#[test]
fn attr_type_round_trip() {
    let mut attr = attr_init();
    attr_settype(&mut attr, PTHREAD_MUTEX_ERRORCHECK).unwrap();
    assert_eq!(attr_gettype(&attr), PTHREAD_MUTEX_ERRORCHECK);
    attr_settype(&mut attr, PTHREAD_MUTEX_RECURSIVE).unwrap();
    assert_eq!(attr_gettype(&attr), PTHREAD_MUTEX_RECURSIVE);
    attr_settype(&mut attr, PTHREAD_MUTEX_NORMAL).unwrap();
    assert_eq!(attr_gettype(&attr), PTHREAD_MUTEX_NORMAL);
}

#[test]
fn attr_rejects_a_bad_type() {
    let mut attr = attr_init();
    for kind in [-1, 3, 4, 100] {
        assert_eq!(attr_settype(&mut attr, kind), Err(EINVAL));
    }
}

#[test]
fn attr_pshared_private_is_accepted() {
    let kernel = kernel();
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, crate::limits::PTHREAD_PROCESS_PRIVATE),
        Ok(())
    );
    assert_eq!(
        attr_getpshared(&attr),
        crate::limits::PTHREAD_PROCESS_PRIVATE
    );
}

#[test]
fn attr_pshared_shared_is_enotsup_without_pg4() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, crate::limits::PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
    assert_eq!(
        attr_getpshared(&attr),
        crate::limits::PTHREAD_PROCESS_PRIVATE
    );
}

#[test]
fn attr_pshared_shared_is_accepted_with_pg4() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, crate::limits::PTHREAD_PROCESS_SHARED),
        Ok(())
    );
    assert_eq!(
        attr_getpshared(&attr),
        crate::limits::PTHREAD_PROCESS_SHARED
    );
}

#[test]
fn attr_rejects_a_bad_pshared() {
    let kernel = kernel();
    let mut attr = attr_init();
    assert_eq!(attr_setpshared(&kernel, &mut attr, 2), Err(EINVAL));
    assert_eq!(attr_setpshared(&kernel, &mut attr, -1), Err(EINVAL));
}

#[test]
fn attr_protocol_none_and_inherit() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut attr = attr_init();
    assert_eq!(
        attr_setprotocol(&kernel, &mut attr, PTHREAD_PRIO_NONE),
        Ok(())
    );
    assert_eq!(attr_getprotocol(&attr), PTHREAD_PRIO_NONE);
    assert_eq!(
        attr_setprotocol(&kernel, &mut attr, PTHREAD_PRIO_INHERIT),
        Err(ENOTSUP)
    );
    assert_eq!(
        attr_setprotocol(&kernel, &mut attr, PTHREAD_PRIO_PROTECT),
        Err(ENOTSUP)
    );
    assert_eq!(attr_setprotocol(&kernel, &mut attr, 7), Err(EINVAL));
}

#[test]
fn attr_protocol_inherit_with_pg6() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    let mut attr = attr_init();
    assert_eq!(
        attr_setprotocol(&kernel, &mut attr, PTHREAD_PRIO_INHERIT),
        Ok(())
    );
    assert_eq!(attr_getprotocol(&attr), PTHREAD_PRIO_INHERIT);
}

#[test]
fn attr_robust_round_trip() {
    let mut attr = attr_init();
    assert_eq!(attr_setrobust(&mut attr, 1), Ok(()));
    assert_eq!(attr_getrobust(&attr), 1);
    assert_eq!(attr_setrobust(&mut attr, 0), Ok(()));
    assert_eq!(attr_getrobust(&attr), 0);
    assert_eq!(attr_setrobust(&mut attr, 2), Err(EINVAL));
    assert_eq!(attr_setrobust(&mut attr, -1), Err(EINVAL));
}

#[test]
fn normal_lock_free_is_one_cas() {
    let kernel = kernel();
    let mutex = Mutex::new();
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.lock_value(), EBUSY.0 as u32);
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.lock_value(), 0);
}

#[test]
fn normal_trylock_success_and_busy() {
    let kernel = kernel();
    let mutex = Mutex::new();
    assert_eq!(trylock(&kernel, &mutex), Ok(()));
    assert_eq!(trylock(&kernel, &mutex), Err(EBUSY));
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
}

#[test]
fn recursive_lock_counts_nesting() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&recursive_attr()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.count(), 1);
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.count(), 0);
    assert_ne!(mutex.lock_value(), 0);
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.lock_value(), 0);
}

#[test]
fn recursive_trylock_counts_too() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&recursive_attr()));
    assert_eq!(trylock(&kernel, &mutex), Ok(()));
    assert_eq!(trylock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.count(), 1);
}

#[test]
fn recursive_at_int_max_returns_eagain() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&recursive_attr()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    mutex.set_count(i32::MAX);
    assert_eq!(lock(&kernel, &mutex), Err(EAGAIN));
}

#[test]
fn recursive_unlock_by_another_tid_is_eperm() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&recursive_attr()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    crate::thread::current_pthread(&kernel)
        .tid
        .store(200, Ordering::Release);
    assert_eq!(unlock(&kernel, &mutex), Err(EPERM));
}

#[test]
fn errorcheck_self_lock_is_edeadlk() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&errorcheck_attr()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    assert_eq!(lock(&kernel, &mutex), Err(EDEADLK));
}

#[test]
fn errorcheck_self_trylock_is_ebusy_in_the_owner_path() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&errorcheck_attr()));
    assert_eq!(trylock(&kernel, &mutex), Ok(()));
    assert_eq!(trylock(&kernel, &mutex), Err(EBUSY));
}

#[test]
fn errorcheck_unlock_by_another_tid_is_eperm() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&errorcheck_attr()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    crate::thread::current_pthread(&kernel)
        .tid
        .store(201, Ordering::Release);
    assert_eq!(unlock(&kernel, &mutex), Err(EPERM));
}

#[test]
fn normal_unlock_does_not_check_the_owner() {
    let kernel = kernel();
    let mutex = Mutex::new();
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    crate::thread::current_pthread(&kernel)
        .tid
        .store(202, Ordering::Release);
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
}

#[test]
fn robust_trylock_after_owner_death_returns_eownerdead() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&robust_attr()));
    mutex
        .lock_word()
        .store(OWNER_DIED as i32, Ordering::Release);
    assert_eq!(trylock(&kernel, &mutex), Err(EOWNERDEAD));
    assert_eq!(
        mutex.lock_value() & 0x3fff_ffff,
        crate::thread::current_pthread(&kernel)
            .tid
            .load(Ordering::Acquire) as u32
    );
}

#[test]
fn robust_consistent_clears_the_owner_died_bit() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&robust_attr()));
    mutex
        .lock_word()
        .store(OWNER_DIED as i32, Ordering::Release);
    assert_eq!(trylock(&kernel, &mutex), Err(EOWNERDEAD));
    assert_eq!(consistent(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.lock_value() & OWNER_DIED, 0);
}

#[test]
fn robust_consistent_on_a_non_robust_mutex_is_einval() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, None);
    assert_eq!(consistent(&kernel, &mutex), Err(EINVAL));
}

#[test]
fn robust_unlock_without_consistent_is_not_recoverable() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&robust_attr()));
    mutex
        .lock_word()
        .store(OWNER_DIED as i32, Ordering::Release);
    assert_eq!(trylock(&kernel, &mutex), Err(EOWNERDEAD));
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.lock_value() as i32, MUTEX_UNRECOVERABLE as i32);
    assert_eq!(trylock(&kernel, &mutex), Err(ENOTRECOVERABLE));
}

#[test]
fn robust_trylock_pushes_a_node() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&robust_attr()));
    let pthread = crate::thread::current_pthread(&kernel);
    let head = raw::robust_head_addr(pthread);
    assert_eq!(trylock(&kernel, &mutex), Ok(()));
    assert_ne!(raw::robust_head(pthread), head);
    assert_eq!(
        raw::robust_head(pthread),
        &mutex as *const Mutex as usize + 32
    );
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
    assert_eq!(raw::robust_head(pthread), head);
}

#[test]
fn robust_consistent_by_another_tid_is_eperm() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&robust_attr()));
    mutex
        .lock_word()
        .store(OWNER_DIED as i32, Ordering::Release);
    assert_eq!(trylock(&kernel, &mutex), Err(EOWNERDEAD));
    crate::thread::current_pthread(&kernel)
        .tid
        .store(303, Ordering::Release);
    assert_eq!(consistent(&kernel, &mutex), Err(EPERM));
}

#[test]
fn timedlock_past_deadline_does_not_call_the_kernel() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    init(&mut mutex, Some(&errorcheck_attr()));
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    // A second thread would block; here the owner is the caller, so use a normal mutex held by
    // another tid to reach the blocking path.
    let held = Mutex::new();
    held.lock_word().store(999, Ordering::Release);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(timedlock(&kernel, &held, Some(at)), Err(ETIMEDOUT));
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn timedlock_future_deadline_waits_then_times_out() {
    let kernel = kernel();
    let held = Mutex::new();
    held.lock_word().store(999, Ordering::Release);
    kernel.set_wait_result(ETIMEDOUT.0 as i32);
    let at = Timespec {
        tv_sec: 100,
        tv_nsec: 0,
    };
    assert_eq!(timedlock(&kernel, &held, Some(at)), Err(ETIMEDOUT));
    assert_eq!(kernel.wait_calls(), 1);
}

#[test]
fn timedlock_without_a_deadline_uses_a_forever_wait() {
    let kernel = kernel();
    let held = Mutex::new();
    held.lock_word().store(999, Ordering::Release);
    kernel.set_wait_result(ETIMEDOUT.0 as i32);
    assert_eq!(timedlock(&kernel, &held, None), Err(ETIMEDOUT));
    assert_eq!(kernel.last_timeout(), u64::MAX);
}

#[test]
fn a_pi_mutex_is_enotsup() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut mutex = Mutex::new();
    init(&mut mutex, None);
    mutex.set_type(MUTEX_TYPE_PI);
    assert_eq!(lock(&kernel, &mutex), Err(ENOTSUP));
}

#[test]
fn prioceiling_calls_are_enotsup() {
    let mutex = Mutex::new();
    assert_eq!(getprioceiling(&mutex), Err(ENOTSUP));
    assert_eq!(setprioceiling(&mutex, 10), Err(ENOTSUP));
}

#[test]
fn destroy_is_idempotent() {
    let kernel = kernel();
    let mutex = Mutex::new();
    assert_eq!(destroy(&kernel, &mutex), Ok(()));
    assert_eq!(destroy(&kernel, &mutex), Ok(()));
}

#[test]
fn waiters_word_and_lock_word_are_the_documented_slots() {
    let mutex = Mutex::new();
    assert_eq!(
        waiters_word_of(&mutex) as *const AtomicI32 as usize,
        &mutex.words[2] as *const AtomicU32 as usize
    );
    assert_eq!(
        lock_word_of(&mutex) as *const AtomicI32 as usize,
        &mutex.words[1] as *const AtomicU32 as usize
    );
    assert_eq!(type_of(&mutex), 0);
}

#[test]
fn the_type_word_carries_robust_and_shared_bits() {
    let mut mutex = Mutex::new();
    mutex.set_type(MUTEX_TYPE_ROBUST | MUTEX_TYPE_SHARED);
    assert!(mutex.is_robust());
    assert!(mutex.is_shared());
    assert_eq!(mutex.kind(), MUTEX_TYPE_ROBUST);
}

#[test]
fn a_held_normal_mutex_has_ebusy_as_the_owner_word() {
    let kernel = kernel();
    let mutex = Mutex::new();
    assert_eq!(lock(&kernel, &mutex), Ok(()));
    assert_eq!(mutex.lock_value(), 16);
    assert_eq!(mutex.lock_value() & crate::limits::OWNER_MASK, 16);
    assert_eq!(unlock(&kernel, &mutex), Ok(()));
}

#[test]
fn attr_destroy_and_init_are_stable() {
    let attr = attr_init();
    attr_destroy(&attr);
    assert_eq!(attr.attr, 0);
}
