// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::limits::{EBUSY, EINVAL, ENOTSUP, ETIMEDOUT};
use core::sync::atomic::Ordering;

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

#[test]
fn size_and_static_initialiser() {
    assert_eq!(core::mem::size_of::<RwLock>(), 56);
    assert_eq!(core::mem::size_of::<RwLockAttr>(), 8);
    let lock = RwLock::new();
    assert_eq!(lock.value(), 0);
    assert!(!lock.is_shared());
}

#[test]
fn init_with_a_private_attr() {
    let mut lock = RwLock::new();
    let attr = attr_init();
    init(&mut lock, Some(&attr));
    assert_eq!(lock.value(), 0);
    assert!(!lock.is_shared());
}

#[test]
fn init_with_a_shared_attr_sets_word_two() {
    let mut lock = RwLock::new();
    let mut attr = attr_init();
    attr.shared = 1;
    init(&mut lock, Some(&attr));
    assert!(lock.is_shared());
    assert_eq!(lock.words[2].load(Ordering::Relaxed), 128);
}

#[test]
fn one_reader_takes_the_lock() {
    let lock = RwLock::new();
    assert_eq!(tryrdlock(&lock), Ok(()));
    assert_eq!(reader_count(&lock), 1);
    assert_eq!(unlock(&kernel(), &lock), Ok(()));
    assert_eq!(lock.value(), 0);
}

#[test]
fn many_readers_are_counted() {
    let lock = RwLock::new();
    for _ in 0..10 {
        assert_eq!(tryrdlock(&lock), Ok(()));
    }
    assert_eq!(reader_count(&lock), 10);
}

#[test]
fn a_writer_excludes_readers() {
    let lock = RwLock::new();
    assert_eq!(trywrlock(&lock), Ok(()));
    assert_eq!(tryrdlock(&lock), Err(EBUSY));
    assert_eq!(trywrlock(&lock), Err(EBUSY));
    assert_eq!(lock.value(), 0x7fff_ffff);
}

#[test]
fn readers_exclude_a_writer() {
    let lock = RwLock::new();
    assert_eq!(tryrdlock(&lock), Ok(()));
    assert_eq!(trywrlock(&lock), Err(EBUSY));
}

#[test]
fn unlock_a_write_lock_frees_it() {
    let kernel = kernel();
    let lock = RwLock::new();
    assert_eq!(trywrlock(&lock), Ok(()));
    assert_eq!(unlock(&kernel, &lock), Ok(()));
    assert_eq!(lock.value(), 0);
}

#[test]
fn rdlock_and_wrlock_wrappers_work_on_a_free_lock() {
    let kernel = kernel();
    let lock = RwLock::new();
    assert_eq!(rdlock(&kernel, &lock), Ok(()));
    assert_eq!(unlock(&kernel, &lock), Ok(()));
    assert_eq!(wrlock(&kernel, &lock), Ok(()));
    assert_eq!(unlock(&kernel, &lock), Ok(()));
}

#[test]
fn a_timedrdlock_past_deadline_does_not_wait() {
    let kernel = kernel();
    let lock = RwLock::new();
    assert_eq!(trywrlock(&lock), Ok(()));
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(timedrdlock(&kernel, &lock, Some(at)), Err(ETIMEDOUT));
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn a_timedwrlock_future_deadline_waits_then_times_out() {
    let kernel = kernel();
    let lock = RwLock::new();
    assert_eq!(tryrdlock(&lock), Ok(()));
    kernel.set_wait_result(ETIMEDOUT.0 as i32);
    let at = Timespec {
        tv_sec: 100,
        tv_nsec: 0,
    };
    assert_eq!(timedwrlock(&kernel, &lock, Some(at)), Err(ETIMEDOUT));
    assert_eq!(kernel.wait_calls(), 1);
}

#[test]
fn reader_saturation_returns_eagain() {
    let lock = RwLock::new();
    lock.lock_word()
        .store(0x7fff_fffeu32 as i32, Ordering::Release);
    assert_eq!(tryrdlock(&lock), Err(EAGAIN));
}

#[test]
fn attr_pshared_private_and_shared() {
    let kernel = crate::test_support::MockKernel::new();
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, PTHREAD_PROCESS_PRIVATE),
        Ok(())
    );
    assert_eq!(attr_getpshared(&attr), PTHREAD_PROCESS_PRIVATE);
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, PTHREAD_PROCESS_SHARED),
        Ok(())
    );
    assert_eq!(attr_getpshared(&attr), PTHREAD_PROCESS_SHARED);
}

#[test]
fn attr_pshared_shared_is_enotsup_without_pg4() {
    let kernel = crate::test_support::MockKernel::with_caps(crate::kernel::KernelCaps::minimal());
    let mut attr = attr_init();
    assert_eq!(
        attr_setpshared(&kernel, &mut attr, PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
}

#[test]
fn attr_pshared_rejects_a_bad_value() {
    let kernel = kernel();
    let mut attr = attr_init();
    assert_eq!(attr_setpshared(&kernel, &mut attr, -1), Err(EINVAL));
    assert_eq!(attr_setpshared(&kernel, &mut attr, 2), Err(EINVAL));
}

#[test]
fn the_np_kind_setter_is_accepted_and_ignored() {
    let mut attr = attr_init();
    assert_eq!(attr_setkind_np(&mut attr, 3), Ok(()));
    assert_eq!(attr_getkind_np(&attr), 0);
    assert_eq!(attr_setkind_np(&mut attr, -1), Err(EINVAL));
}

#[test]
fn destroy_is_a_no_op() {
    let lock = RwLock::new();
    destroy(&lock);
    assert_eq!(lock.value(), 0);
}

#[test]
fn attr_destroy_is_a_no_op() {
    let attr = attr_init();
    attr_destroy(&attr);
    assert_eq!(attr.shared, 0);
}
