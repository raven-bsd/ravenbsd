// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-kernel lifecycle tests: create/join/detach, the stack ledger and the exit/reclaim race.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_thread::kernel::{KernelCaps, ThreadKernel};
use raven_libc_thread::limits::PTHREAD_STACK_MIN;
use raven_libc_thread::thread::attr;
use support::{HostKernel, init_main, seed_from_env};

fn kernel() -> HostKernel {
    HostKernel::new(KernelCaps::full(), seed_from_env(1))
}

fn small_attr() -> attr::Attr {
    let mut attr = attr::init();
    attr.set_stacksize_raw(PTHREAD_STACK_MIN * 4);
    attr
}

#[test]
fn init_builds_the_main_tcb_and_the_tls_block() {
    let _serial = support::serial();
    let kernel = kernel();
    let tcb = init_main(&kernel, 0x1000_0000, 0x1_0000);
    let pthread = raven_libc_thread::thread::current_pthread(&kernel);
    assert_eq!(pthread.self_ptr, tcb);
    assert!(pthread.tid.load(Ordering::Acquire) > 0);
    assert_eq!(kernel.violations(), Vec::<String>::new());
}

#[test]
fn create_and_join_returns_the_result() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(arg: usize) -> usize {
        arg.wrapping_add(7)
    }
    let thread = raven_libc_thread::thread::create::create(&kernel, Some(&small_attr()), body, 35)
        .expect("create");
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(42)
    );
}

#[test]
fn create_with_the_default_attribute_works() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        1
    }
    let thread = raven_libc_thread::thread::create::create(&kernel, None, body, 0).expect("create");
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(1)
    );
}

#[test]
fn detach_and_exit_free_the_stack_once() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    static DONE: AtomicUsize = AtomicUsize::new(0);
    fn body(_: usize) -> usize {
        DONE.fetch_add(1, Ordering::AcqRel);
        0
    }
    let thread = raven_libc_thread::thread::create::create(&kernel, Some(&small_attr()), body, 0)
        .expect("create");
    assert_eq!(
        raven_libc_thread::thread::join::detach(&kernel, thread),
        Ok(())
    );
    let mut spins = 0;
    while DONE.load(Ordering::Acquire) == 0 && spins < 10_000 {
        kernel.yield_now();
        spins += 1;
    }
    assert_eq!(DONE.load(Ordering::Acquire), 1);
    let mut spins = 0;
    while !kernel.is_dead(thread) && spins < 10_000 {
        kernel.yield_now();
        spins += 1;
    }
    assert!(kernel.is_dead(thread));
    assert_eq!(kernel.violations(), Vec::<String>::new());
}

#[test]
fn two_hundred_create_join_cycles_leak_nothing() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        0
    }
    let attr = small_attr();
    for _iteration in 0..200 {
        let thread = raven_libc_thread::thread::create::create(&kernel, Some(&attr), body, 0)
            .expect("create");
        assert_eq!(
            raven_libc_thread::thread::join::join(&kernel, thread),
            Ok(0)
        );
    }
    assert_eq!(kernel.violations(), Vec::<String>::new());
}

#[test]
fn join_of_a_null_thread_is_esrch() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, 0),
        Err(raven_libc_thread::limits::ESRCH)
    );
}

#[test]
fn two_threads_run_concurrently_and_join() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    fn body(_: usize) -> usize {
        COUNTER.fetch_add(1, Ordering::AcqRel);
        0
    }
    let first = raven_libc_thread::thread::create::create(&kernel, Some(&small_attr()), body, 0)
        .expect("first");
    let second = raven_libc_thread::thread::create::create(&kernel, Some(&small_attr()), body, 0)
        .expect("second");
    assert_eq!(raven_libc_thread::thread::join::join(&kernel, first), Ok(0));
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, second),
        Ok(0)
    );
    assert_eq!(COUNTER.load(Ordering::Acquire), 2);
}

#[test]
fn a_joinable_thread_is_not_freed_before_it_is_dead() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        9
    }
    let thread = raven_libc_thread::thread::create::create(&kernel, Some(&small_attr()), body, 0)
        .expect("create");
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(9)
    );
    assert!(kernel.is_dead(thread));
    assert_eq!(kernel.violations(), Vec::<String>::new());
}

#[test]
fn the_scheduler_steps_are_recorded() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        0
    }
    let thread = raven_libc_thread::thread::create::create(&kernel, Some(&small_attr()), body, 0)
        .expect("create");
    let _ = raven_libc_thread::thread::join::join(&kernel, thread);
    assert!(kernel.sched().steps() > 0);
    assert!(!kernel.sched().deadlocked());
}
