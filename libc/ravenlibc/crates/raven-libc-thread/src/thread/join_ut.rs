// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::Ordering;

#[test]
fn a_null_thread_is_esrch() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(join(&kernel, 0), Err(ESRCH));
    assert_eq!(detach(&kernel, 0), Err(ESRCH));
    assert_eq!(tryjoin(&kernel, 0), Err(ESRCH));
}

#[test]
fn joining_self_is_edeadlk() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(join(&kernel, kernel.tcb()), Err(crate::limits::EDEADLK));
}

#[test]
fn detaching_twice_is_einval() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let peer = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&peer);
    pthread
        .detach_state
        .store(raven_libc_tls::tcb::DT_JOINABLE, Ordering::Release);
    pthread.tid.store(200, Ordering::Release);
    assert_eq!(detach(&kernel, peer.tcb()), Ok(()));
    assert_eq!(detach(&kernel, peer.tcb()), Err(EINVAL));
}

#[test]
fn joining_a_detached_thread_is_einval() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let peer = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&peer);
    pthread.tid.store(200, Ordering::Release);
    pthread
        .detach_state
        .store(raven_libc_tls::tcb::DT_DETACHED, Ordering::Release);
    assert_eq!(join(&kernel, peer.tcb()), Err(EINVAL));
}

#[test]
fn a_stale_thread_is_esrch() {
    let kernel = crate::test_support::MockKernel::new();
    let peer = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&peer);
    pthread.tid.store(-1, Ordering::Release);
    assert_eq!(join(&kernel, peer.tcb()), Err(ESRCH));
    assert_eq!(detach(&kernel, peer.tcb()), Err(ESRCH));
}

#[test]
fn self_and_equal() {
    let kernel = crate::test_support::MockKernel::new();
    let current = self_thread(&kernel);
    assert!(equal(current, current));
    assert!(!equal(current, current.wrapping_add(1)));
}

#[test]
fn tryjoin_reports_busy_for_a_live_joinable_thread() {
    let kernel = crate::test_support::MockKernel::new();
    let peer = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&peer);
    pthread.tid.store(200, Ordering::Release);
    pthread
        .detach_state
        .store(raven_libc_tls::tcb::DT_JOINABLE, Ordering::Release);
    assert_eq!(tryjoin(&kernel, peer.tcb()), Err(EBUSY));
}

#[test]
fn getcpuclockid_is_enotsup() {
    assert_eq!(getcpuclockid(1), Err(ENOTSUP));
}
