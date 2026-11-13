// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use crate::limits::{EINVAL, ENOTSUP};

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

#[test]
fn size_and_static_initialiser() {
    assert_eq!(core::mem::size_of::<Barrier>(), 32);
    assert_eq!(core::mem::size_of::<BarrierAttr>(), 4);
    let barrier = Barrier::new();
    assert!(
        barrier
            .words
            .iter()
            .all(|word| word.load(Ordering::Relaxed) == 0)
    );
    assert_eq!(barrier.limit(), 0);
}

#[test]
fn init_sets_the_limit_word() {
    let mut barrier = Barrier::new();
    assert_eq!(init(&mut barrier, None, 4), Ok(()));
    assert_eq!(barrier.limit(), 3);
}

#[test]
fn a_zero_count_is_einval() {
    let mut barrier = Barrier::new();
    assert_eq!(init(&mut barrier, None, 0), Err(EINVAL));
}

#[test]
fn a_shared_attr_sets_the_sign_bit() {
    let mut barrier = Barrier::new();
    let mut attr = attr_init();
    attr_setpshared(&kernel(), &mut attr, crate::limits::PTHREAD_PROCESS_SHARED).unwrap();
    init(&mut barrier, Some(&attr), 2).unwrap();
    assert!(barrier.limit() < 0);
}

#[test]
fn a_one_thread_barrier_returns_serial() {
    let kernel = kernel();
    let mut barrier = Barrier::new();
    init(&mut barrier, None, 1).unwrap();
    assert_eq!(
        wait_barrier(&kernel, &barrier),
        Ok(PTHREAD_BARRIER_SERIAL_THREAD)
    );
}

#[test]
fn a_shared_barrier_wait_is_enotsup() {
    let kernel = kernel();
    let barrier = Barrier::new();
    barrier.words[2].store((i32::MIN | 1) as u32, Ordering::Release);
    assert_eq!(wait_barrier(&kernel, &barrier), Err(ENOTSUP));
}

#[test]
fn attr_pshared_round_trip() {
    let kernel = kernel();
    let mut attr = attr_init();
    assert_eq!(attr_setpshared(&kernel, &mut attr, 0), Ok(()));
    assert_eq!(attr_getpshared(&attr), 0);
    assert_eq!(attr_setpshared(&kernel, &mut attr, 1), Ok(()));
    assert_eq!(attr_getpshared(&attr), 1);
    assert_eq!(attr_setpshared(&kernel, &mut attr, 2), Err(EINVAL));
}

#[test]
fn attr_pshared_shared_is_enotsup_without_pg4() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let mut attr = attr_init();
    assert_eq!(attr_setpshared(&kernel, &mut attr, 1), Err(ENOTSUP));
}

#[test]
fn destroy_is_stable() {
    let kernel = kernel();
    let barrier = Barrier::new();
    assert_eq!(destroy(&kernel, &barrier), Ok(()));
    let attr = attr_init();
    attr_destroy(&attr);
}

#[test]
fn the_instance_helpers_read_the_published_address() {
    let mut barrier = Barrier::new();
    init(&mut barrier, None, 2).unwrap();
    assert_eq!(instance_count(&barrier), 0);
}
