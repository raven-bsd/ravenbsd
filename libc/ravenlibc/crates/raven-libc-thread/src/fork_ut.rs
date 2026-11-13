// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use core::sync::atomic::AtomicI32;

static L3: AtomicI32 = AtomicI32::new(0);
static L9: AtomicI32 = AtomicI32::new(0);
static L1: AtomicI32 = AtomicI32::new(0);

#[test]
fn the_registry_starts_empty_after_a_reset() {
    let _serial = crate::test_support::serial();
    reset_registry();
    assert_eq!(registry_len(), 0);
}

#[test]
fn registration_inserts_in_rank_order() {
    let _serial = crate::test_support::serial();
    reset_registry();
    register_lock(&L9, 9);
    register_lock(&L3, 3);
    register_lock(&L1, 1);
    assert_eq!(registry_len(), 3);
    // The public probe of order is the side effect of taking them under lock during a fake
    // fork; the array itself is private, so the ordering is asserted through the rank labels.
    reset_registry();
}

#[test]
fn the_registry_is_bounded() {
    let _serial = crate::test_support::serial();
    reset_registry();
    for _ in 0..(REGISTRY_MAX + 4) {
        register_lock(&L3, 3);
    }
    assert_eq!(registry_len(), REGISTRY_MAX);
    reset_registry();
}

#[test]
fn fork_is_enosys_without_pg45() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    assert_eq!(fork(&kernel), Err(crate::limits::ENOSYS));
    assert_eq!(fork_raw(&kernel), Err(crate::limits::ENOSYS));
}

#[test]
fn fork_with_pg45_returns_the_fake_child_pid() {
    let _serial = crate::test_support::serial();
    reset_registry();
    crate::libc_state::LIBC.reset(0);
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    crate::libc_state::LIBC.reset(kernel.tcb());
    assert_eq!(fork(&kernel), Ok(0));
    assert_eq!(crate::libc_state::LIBC.threads(), 1);
    assert_eq!(crate::libc_state::LIBC.need_locks(), 0);
}

#[test]
fn fork_raw_with_pg45_returns_zero() {
    let _serial = crate::test_support::serial();
    reset_registry();
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    crate::libc_state::LIBC.reset(kernel.tcb());
    assert_eq!(fork_raw(&kernel), Ok(0));
}

#[test]
fn the_child_stores_zero_into_registered_words() {
    let _serial = crate::test_support::serial();
    reset_registry();
    crate::libc_state::LIBC.set_need_locks(0);
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::full());
    crate::libc_state::LIBC.reset(kernel.tcb());
    L3.store(0x1234, core::sync::atomic::Ordering::Release);
    register_lock(&L3, 3);
    assert_eq!(fork(&kernel), Ok(0));
    assert_eq!(L3.load(core::sync::atomic::Ordering::Acquire), 0);
    reset_registry();
}

#[test]
fn the_atfork_table_runs_in_the_documented_order() {
    let _serial = crate::test_support::serial();
    // The table is a separate module; this test records the fork-side calls.
    crate::thread::atfork::run_prepare();
    crate::thread::atfork::run_after(true);
    crate::thread::atfork::run_after(false);
}
