// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::limits::ETIMEDOUT;

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

#[test]
fn error_codes_are_the_c11_values() {
    assert_eq!(THRD_SUCCESS, 0);
    assert_eq!(THRD_BUSY, 1);
    assert_eq!(THRD_ERROR, 2);
    assert_eq!(THRD_NOMEM, 3);
    assert_eq!(THRD_TIMEDOUT, 4);
}

#[test]
fn mtx_init_and_lock_unlock() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    assert_eq!(mtx_init(&mut mutex, 0), THRD_SUCCESS);
    assert_eq!(mtx_lock(&kernel, &mutex), THRD_SUCCESS);
    assert_eq!(mtx_trylock(&kernel, &mutex), THRD_BUSY);
    assert_eq!(mtx_unlock(&kernel, &mutex), THRD_SUCCESS);
    assert_eq!(mtx_trylock(&kernel, &mutex), THRD_SUCCESS);
    assert_eq!(mtx_unlock(&kernel, &mutex), THRD_SUCCESS);
}

#[test]
fn mtx_init_rejects_a_negative_kind() {
    let mut mutex = Mutex::new();
    assert_eq!(mtx_init(&mut mutex, -1), THRD_ERROR);
}

#[test]
fn mtx_timedlock_past_deadline_is_timedout() {
    let kernel = kernel();
    let held = Mutex::new();
    held.lock_word()
        .store(999, core::sync::atomic::Ordering::Release);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(mtx_timedlock(&kernel, &held, at), THRD_TIMEDOUT);
}

#[test]
fn cnd_init_wait_signal_and_timedwait() {
    let kernel = kernel();
    let mut mutex = Mutex::new();
    mtx_init(&mut mutex, 0);
    let mut cond = crate::cond::Cond::new();
    assert_eq!(cnd_init(&mut cond), THRD_SUCCESS);
    // A signal with no waiter is a success.
    assert_eq!(cnd_signal(&kernel, &cond), THRD_SUCCESS);
    assert_eq!(cnd_broadcast(&kernel, &cond), THRD_SUCCESS);
    assert_eq!(mtx_lock(&kernel, &mutex), THRD_SUCCESS);
    // A past deadline on the wait path.
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let _ = cnd_timedwait(&kernel, &cond, &mutex, at);
    assert_eq!(mtx_unlock(&kernel, &mutex), THRD_SUCCESS);
    cnd_destroy(&cond);
}

#[test]
fn mtx_destroy_is_a_no_op() {
    let mutex = Mutex::new();
    mtx_destroy(&mutex);
    assert_eq!(
        mutex.words[0].load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn thrd_equal_and_current() {
    let kernel = kernel();
    let current = thrd_current(&kernel);
    assert!(thrd_equal(current, current));
    assert!(!thrd_equal(current, current.wrapping_add(1)));
}

#[test]
fn thrd_yield_hits_the_kernel() {
    let kernel = kernel();
    thrd_yield(&kernel);
    thrd_sleep(
        &kernel,
        Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        None,
    );
}

#[test]
fn tss_create_get_set_delete() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let (code, key) = tss_create(&kernel, None);
    assert_eq!(code, THRD_SUCCESS);
    assert_eq!(tss_set(&kernel, key, 0x42), THRD_SUCCESS);
    assert_eq!(tss_get(&kernel, key), 0x42);
    tss_delete(&kernel, key);
    assert_eq!(tss_set(&kernel, key, 1), THRD_ERROR);
}

#[test]
fn call_once_runs_once() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let flag = Once::new();
    call_once(&kernel, &flag, || {});
    assert_eq!(flag.word.load(core::sync::atomic::Ordering::Acquire), 2);
}

#[test]
fn thrd_join_reports_esrch_for_a_null_thread() {
    let kernel = kernel();
    assert_eq!(thrd_join(&kernel, 0), Err(THRD_ERROR));
}

#[test]
fn thrd_detach_reports_esrch_for_a_null_thread() {
    let kernel = kernel();
    assert_eq!(thrd_detach(&kernel, 0), Err(THRD_ERROR));
}

#[test]
fn the_joinable_constant_is_the_pthread_one() {
    assert_eq!(THRD_JOINABLE, 0);
    assert_eq!(ETIMEDOUT.0, 110);
}
