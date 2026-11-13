// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The cooperative lock registry and the child-side fork repair (H-21, A-thr-9).

mod support;

use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use raven_libc_thread::kernel::KernelCaps;
use support::{HostKernel, init_main, seed_from_env};

static A: AtomicI32 = AtomicI32::new(0);
static B: AtomicI32 = AtomicI32::new(0);
static C: AtomicI32 = AtomicI32::new(0);
static D: AtomicI32 = AtomicI32::new(0);
static E: AtomicI32 = AtomicI32::new(0);
static F: AtomicI32 = AtomicI32::new(0);
static G: AtomicI32 = AtomicI32::new(0);
static H: AtomicI32 = AtomicI32::new(0);
static ORDER: AtomicUsize = AtomicUsize::new(0);
static PREPARE_LAST: AtomicUsize = AtomicUsize::new(0);
static CHILD_FIRST: AtomicUsize = AtomicUsize::new(0);
static CHILD_SAW_RESET: AtomicUsize = AtomicUsize::new(0);

fn prepare() {
    PREPARE_LAST.store(ORDER.fetch_add(1, Ordering::AcqRel) + 1, Ordering::Release);
}

fn child() {
    let position = ORDER.fetch_add(1, Ordering::AcqRel) + 1;
    CHILD_FIRST.store(position, Ordering::Release);
    if raven_libc_thread::state_probe::threads() == 1
        && raven_libc_thread::state_probe::need_locks() == 0
        && raven_libc_thread::raw::list_state() == 0
    {
        CHILD_SAW_RESET.store(1, Ordering::Release);
    }
}

fn locks() -> [&'static AtomicI32; 8] {
    [&A, &B, &C, &D, &E, &F, &G, &H]
}

#[test]
fn the_child_resets_every_registered_word_and_every_other_tid() {
    let _serial = support::serial();
    raven_libc_thread::fork::reset_registry();
    let kernel = HostKernel::new(KernelCaps::full(), seed_from_env(21));
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    for (index, lock) in locks().iter().enumerate() {
        lock.store(900 + index as i32, Ordering::Release);
        raven_libc_thread::fork::register_lock(lock, (index + 1) as u8);
    }
    raven_libc_thread::state_probe::set_threads(9);
    raven_libc_thread::fork::fork_child(&kernel);
    for lock in locks() {
        assert_eq!(lock.load(Ordering::Acquire), 0);
    }
    assert_eq!(raven_libc_thread::state_probe::threads(), 1);
    assert_eq!(raven_libc_thread::state_probe::need_locks(), 0);
    assert_eq!(raven_libc_thread::raw::list_state(), 0);
}

#[test]
fn free_registered_locks_are_taken_and_released() {
    let _serial = support::serial();
    raven_libc_thread::fork::reset_registry();
    let kernel = HostKernel::new(KernelCaps::full(), seed_from_env(22));
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    for (index, lock) in locks().iter().enumerate() {
        raven_libc_thread::fork::register_lock(lock, (index + 1) as u8);
    }
    raven_libc_thread::state_probe::set_threads(9);
    raven_libc_thread::fork::fork_prepare(&kernel);
    assert_ne!(raven_libc_thread::raw::list_state(), 0);
    raven_libc_thread::fork::fork_parent(&kernel);
    assert_eq!(raven_libc_thread::raw::list_state(), 0);
    for lock in locks() {
        assert_eq!(lock.load(Ordering::Acquire), 0);
    }
}

#[test]
fn fork_runs_the_handlers_in_order_and_after_the_reset() {
    let _serial = support::serial();
    raven_libc_thread::fork::reset_registry();
    raven_libc_thread::thread::atfork::clear();
    ORDER.store(0, Ordering::Release);
    CHILD_SAW_RESET.store(0, Ordering::Release);
    raven_libc_thread::thread::atfork::pthread_atfork(Some(prepare), None, Some(child)).unwrap();
    let kernel = HostKernel::new(KernelCaps::full(), seed_from_env(23));
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(raven_libc_thread::fork::fork(&kernel), Ok(0));
    assert_eq!(PREPARE_LAST.load(Ordering::Acquire), 1);
    assert_eq!(CHILD_FIRST.load(Ordering::Acquire), 2);
    assert_eq!(CHILD_SAW_RESET.load(Ordering::Acquire), 1);
    raven_libc_thread::thread::atfork::clear();
}

#[test]
fn _fork_skips_the_user_handlers() {
    let _serial = support::serial();
    raven_libc_thread::fork::reset_registry();
    raven_libc_thread::thread::atfork::clear();
    ORDER.store(0, Ordering::Release);
    raven_libc_thread::thread::atfork::pthread_atfork(Some(prepare), None, None).unwrap();
    let kernel = HostKernel::new(KernelCaps::full(), seed_from_env(24));
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(raven_libc_thread::fork::fork_raw(&kernel), Ok(0));
    assert_eq!(ORDER.load(Ordering::Acquire), 0);
    raven_libc_thread::thread::atfork::clear();
}

#[test]
fn the_registry_is_rank_sorted_and_bounded() {
    let _serial = support::serial();
    raven_libc_thread::fork::reset_registry();
    for (index, lock) in locks().iter().enumerate() {
        raven_libc_thread::fork::register_lock(lock, (8 - index) as u8);
    }
    assert_eq!(raven_libc_thread::fork::registry_len(), 8);
    for _ in 0..40 {
        raven_libc_thread::fork::register_lock(&A, 5);
    }
    assert_eq!(
        raven_libc_thread::fork::registry_len(),
        raven_libc_thread::limits::REGISTRY_MAX
    );
    raven_libc_thread::fork::reset_registry();
}

#[test]
fn fork_without_pg45_is_enosys_even_with_handlers() {
    let _serial = support::serial();
    raven_libc_thread::fork::reset_registry();
    raven_libc_thread::thread::atfork::clear();
    let kernel = HostKernel::new(
        KernelCaps::full().with(raven_libc_thread::kernel::CapFlag::ProcFork, false),
        seed_from_env(25),
    );
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(
        raven_libc_thread::fork::fork(&kernel),
        Err(raven_libc_thread::limits::ENOSYS)
    );
}
