// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A-thr-3: never unmap a stack on the strength of a user-space flag alone.

mod support;

use raven_libc_thread::kernel::{KernelCaps, ThreadKernel};
use raven_libc_thread::limits::PTHREAD_STACK_MIN;
use support::{HostKernel, init_main, seed_from_env};

fn attr() -> raven_libc_thread::thread::attr::Attr {
    let mut attr = raven_libc_thread::thread::attr::init();
    attr.set_stacksize_raw(PTHREAD_STACK_MIN * 4);
    attr
}

fn run_good(exit_ex: bool) {
    let _serial = support::serial();
    let kernel = HostKernel::new(
        KernelCaps::full().with(raven_libc_thread::kernel::CapFlag::ExitEx, exit_ex),
        seed_from_env(11),
    );
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        0x77
    }
    let thread =
        raven_libc_thread::thread::create::create(&kernel, Some(&attr()), body, 0).unwrap();
    let result = raven_libc_thread::thread::join::join(&kernel, thread);
    assert_eq!(result, Ok(0x77));
    assert!(kernel.is_dead(thread));
    assert_eq!(kernel.violations(), Vec::<String>::new());
    assert_eq!(kernel.mapped_regions(), 0, "exit_ex={exit_ex}");
}

#[test]
fn good_join_with_exit_ex() {
    run_good(true);
}

#[test]
fn good_join_without_exit_ex() {
    run_good(false);
}

#[test]
fn a_wrong_join_that_unmaps_on_exit_word_is_caught() {
    let _serial = support::serial();
    let kernel = HostKernel::new(
        KernelCaps::full().with(raven_libc_thread::kernel::CapFlag::ExitEx, false),
        seed_from_env(12),
    );
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        1
    }
    let thread =
        raven_libc_thread::thread::create::create(&kernel, Some(&attr()), body, 0).unwrap();
    let pthread = raven_libc_thread::raw::tcb(thread);
    let mut spins = 0;
    while pthread.exit_word.load(std::sync::atomic::Ordering::Acquire) == 0 && spins < 100_000 {
        kernel.yield_now();
        spins += 1;
    }
    assert_eq!(
        pthread.exit_word.load(std::sync::atomic::Ordering::Acquire),
        1
    );
    assert!(
        !kernel.is_dead(thread),
        "the exit-before-kernel window must be open"
    );
    let base = pthread.map_base;
    let size = pthread.map_size;
    assert_ne!(base, 0);
    // The deliberately wrong join: unmap as soon as the user-space word is set.
    assert_eq!(kernel.unmap(base, size), Ok(()));
    assert!(
        kernel
            .violations()
            .iter()
            .any(|line| line.contains("A-thr-3")),
        "the wrong join must be detected: {:?}",
        kernel.violations()
    );
    // The mapping is still there; the real join cleans up after the kernel confirms death.
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(1)
    );
    assert_eq!(kernel.mapped_regions(), 0);
}
