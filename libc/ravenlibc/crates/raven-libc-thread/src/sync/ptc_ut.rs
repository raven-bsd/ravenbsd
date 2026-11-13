// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn vmlock_starts_empty() {
    let _serial = crate::test_support::serial();
    assert_eq!(vm_depth(), 0);
}

#[test]
fn vmlock_count_round_trips() {
    let _serial = crate::test_support::serial();
    vm_lock();
    assert_eq!(vm_depth(), 1);
    vm_lock();
    assert_eq!(vm_depth(), 2);
    let kernel = crate::test_support::MockKernel::new();
    vm_unlock(&kernel);
    vm_unlock(&kernel);
    assert_eq!(vm_depth(), 0);
}

#[test]
fn vm_wait_returns_when_no_holder_exists() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    vm_wait(&kernel);
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn ptc_read_lock_is_reentrant() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    acquire_ptc(&kernel);
    acquire_ptc(&kernel);
    release_ptc(&kernel);
    release_ptc(&kernel);
    assert_eq!(crate::rwlock::reader_count(crate::raw::ptc_lock()), 0);
}

#[test]
fn ptc_write_lock_blocks_then_releases() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    inhibit_ptc(&kernel);
    assert_eq!(crate::raw::ptc_lock().value(), 0x7fff_ffff);
    release_ptc(&kernel);
    assert_eq!(crate::raw::ptc_lock().value(), 0);
}
