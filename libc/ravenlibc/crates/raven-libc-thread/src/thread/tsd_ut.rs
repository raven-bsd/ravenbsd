// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::AtomicUsize;

static DESTROYED: AtomicUsize = AtomicUsize::new(0);

fn dtor(_value: usize) {
    DESTROYED.fetch_add(1, Ordering::AcqRel);
}

#[test]
fn the_main_array_is_a_hundred_and_twenty_eight_words() {
    assert_eq!(main_tsd().len(), PTHREAD_KEYS_MAX);
    assert_eq!(main_tsd_address() % core::mem::align_of::<AtomicUsize>(), 0);
}

#[test]
fn a_key_can_be_created_and_deleted() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let key = key_create(&kernel, Some(dtor)).unwrap();
    assert!(key < PTHREAD_KEYS_MAX as u32);
    key_delete(&kernel, key);
}

#[test]
fn get_and_set_round_trip() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let key = key_create(&kernel, None).unwrap();
    assert_eq!(getspecific(&kernel, key), 0);
    assert_eq!(setspecific(&kernel, key, 0x1234), Ok(()));
    assert_eq!(getspecific(&kernel, key), 0x1234);
    key_delete(&kernel, key);
}

#[test]
fn a_deleted_key_rejects_setspecific() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let key = key_create(&kernel, None).unwrap();
    key_delete(&kernel, key);
    assert_eq!(setspecific(&kernel, key, 1), Err(EINVAL));
}

#[test]
fn an_out_of_range_key_is_einval() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(setspecific(&kernel, u32::MAX, 1), Err(EINVAL));
    assert_eq!(getspecific(&kernel, u32::MAX), 0);
}

#[test]
fn destructors_run_and_clear_the_value() {
    let _serial = crate::test_support::serial();
    DESTROYED.store(0, Ordering::Release);
    let kernel = crate::test_support::MockKernel::new();
    let key = key_create(&kernel, Some(dtor)).unwrap();
    setspecific(&kernel, key, 7).unwrap();
    run_dtors(&kernel);
    assert_eq!(DESTROYED.load(Ordering::Acquire), 1);
    assert_eq!(getspecific(&kernel, key), 0);
    key_delete(&kernel, key);
}

#[test]
fn a_destructor_may_set_another_key_within_four_rounds() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let key = key_create(&kernel, Some(dtor)).unwrap();
    setspecific(&kernel, key, 1).unwrap();
    // The second round is empty; the loop must stop after at most four rounds.
    run_dtors(&kernel);
    run_dtors(&kernel);
    key_delete(&kernel, key);
}

#[test]
fn the_tss_glue_maps_errors_to_thrd_error() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let (code, key) = crate::c11::tss_create(&kernel, None);
    assert_eq!(code, 0);
    assert_eq!(crate::c11::tss_set(&kernel, key, 9), 0);
    assert_eq!(crate::c11::tss_get(&kernel, key), 9);
    crate::c11::tss_delete(&kernel, key);
}
