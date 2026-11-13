// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The deterministic-scheduler condition-variable runs (C57 7.2): no lost wakeup, no early wake,
//! broadcast wakes every waiter that began before it, signal wakes exactly one. The 100-seed run
//! is the `sched_100` test below; its per-seed step count is printed by the report.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_thread::cond::Cond;
use raven_libc_thread::kernel::{KernelCaps, ThreadKernel};
use raven_libc_thread::mutex::Mutex;
use support::{HostKernel, init_main, seed_from_env};

static ENTERED: AtomicUsize = AtomicUsize::new(0);
static WOKEN: AtomicUsize = AtomicUsize::new(0);
static ROUNDS_DONE: AtomicUsize = AtomicUsize::new(0);

struct Shared {
    mutex: Mutex,
    cond: Cond,
    waiters: usize,
    rounds: usize,
}

struct WaiterArgs {
    shared: *const Shared,
    kernel: HostKernel,
}

fn waiter(arg: usize) -> usize {
    // SAFETY: `create` passes the leaked `WaiterArgs` address and the structure lives for the
    // whole thread; `shared` is leaked for the whole scenario.
    let args = unsafe { &*(arg as *const WaiterArgs) };
    // SAFETY: the pointer was produced from the leaked `Shared` above.
    let shared = unsafe { &*args.shared };
    let kernel = &args.kernel;
    for _ in 0..shared.rounds {
        raven_libc_thread::mutex::lock(kernel, &shared.mutex).unwrap();
        ENTERED.fetch_add(1, Ordering::AcqRel);
        raven_libc_thread::cond::wait(kernel, &shared.cond, &shared.mutex).unwrap();
        WOKEN.fetch_add(1, Ordering::AcqRel);
        raven_libc_thread::mutex::unlock(kernel, &shared.mutex).unwrap();
        ROUNDS_DONE.fetch_add(1, Ordering::AcqRel);
    }
    0
}

fn start_waiters(kernel: &HostKernel, shared: &'static Shared) -> Vec<usize> {
    let mut threads = Vec::new();
    for _ in 0..shared.waiters {
        let args: &'static WaiterArgs = Box::leak(Box::new(WaiterArgs {
            shared: shared as *const Shared,
            kernel: kernel.clone(),
        }));
        let mut attr = raven_libc_thread::thread::attr::init();
        attr.set_stacksize_raw(raven_libc_thread::limits::PTHREAD_STACK_MIN * 8);
        threads.push(
            raven_libc_thread::thread::create::create(
                kernel,
                Some(&attr),
                waiter,
                args as *const WaiterArgs as usize,
            )
            .expect("waiter"),
        );
    }
    threads
}

fn reset() {
    ENTERED.store(0, Ordering::Release);
    WOKEN.store(0, Ordering::Release);
    ROUNDS_DONE.store(0, Ordering::Release);
}

#[test]
fn broadcast_wakes_every_waiter_that_began_before_it() {
    let _serial = support::serial();
    reset();
    let kernel = HostKernel::new(KernelCaps::full(), seed_from_env(1));
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let shared: &'static Shared = Box::leak(Box::new(Shared {
        mutex: Mutex::new(),
        cond: Cond::new(),
        waiters: 3,
        rounds: 2,
    }));
    let threads = start_waiters(&kernel, shared);
    for round in 0..shared.rounds {
        let mut spins = 0;
        while shared.cond.waiter_count() < shared.waiters && spins < 100_000 {
            kernel.yield_now();
            spins += 1;
        }
        assert_eq!(shared.cond.waiter_count(), shared.waiters, "round {round}");
        raven_libc_thread::cond::broadcast(&kernel, &shared.cond).unwrap();
        let target = (round + 1) * shared.waiters;
        let mut spins = 0;
        while ROUNDS_DONE.load(Ordering::Acquire) < target && spins < 100_000 {
            kernel.yield_now();
            spins += 1;
        }
        assert_eq!(ROUNDS_DONE.load(Ordering::Acquire), target, "round {round}");
    }
    for thread in threads {
        assert_eq!(
            raven_libc_thread::thread::join::join(&kernel, thread),
            Ok(0)
        );
    }
    assert_eq!(
        WOKEN.load(Ordering::Acquire),
        shared.rounds * shared.waiters
    );
    assert!(!kernel.sched().deadlocked());
}

#[test]
fn signal_wakes_exactly_one_of_two_waiters() {
    let _serial = support::serial();
    reset();
    let kernel = HostKernel::new(KernelCaps::full(), seed_from_env(2));
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let shared: &'static Shared = Box::leak(Box::new(Shared {
        mutex: Mutex::new(),
        cond: Cond::new(),
        waiters: 2,
        rounds: 1,
    }));
    let threads = start_waiters(&kernel, shared);
    let mut spins = 0;
    while shared.cond.waiter_count() < 2 && spins < 100_000 {
        kernel.yield_now();
        spins += 1;
    }
    assert_eq!(shared.cond.waiter_count(), 2);
    raven_libc_thread::cond::signal(&kernel, &shared.cond).unwrap();
    spins = 0;
    while WOKEN.load(Ordering::Acquire) == 0 && spins < 100_000 {
        kernel.yield_now();
        spins += 1;
    }
    assert_eq!(WOKEN.load(Ordering::Acquire), 1);
    // Release the second waiter so the join can finish.
    raven_libc_thread::cond::signal(&kernel, &shared.cond).unwrap();
    for thread in threads {
        assert_eq!(
            raven_libc_thread::thread::join::join(&kernel, thread),
            Ok(0)
        );
    }
    assert_eq!(WOKEN.load(Ordering::Acquire), 2);
}

#[test]
fn a_failing_seed_replays_from_the_trace() {
    // The trace is a pure function of the seed; a second run with the same seed produces the
    // same prefix. This is the property `RL_SCHED_SEED` relies on.
    let _serial = support::serial();
    reset();
    let first = HostKernel::new(KernelCaps::full(), 0x1234);
    init_main(&first, 0x1000_0000, 0x1_0000);
    let shared: &'static Shared = Box::leak(Box::new(Shared {
        mutex: Mutex::new(),
        cond: Cond::new(),
        waiters: 2,
        rounds: 1,
    }));
    let threads = start_waiters(&first, shared);
    let mut spins = 0;
    while shared.cond.waiter_count() < 2 && spins < 100_000 {
        first.yield_now();
        spins += 1;
    }
    raven_libc_thread::cond::broadcast(&first, &shared.cond).unwrap();
    for thread in threads {
        let _ = raven_libc_thread::thread::join::join(&first, thread);
    }
    let trace = first.sched().trace();
    assert!(!trace.is_empty());
    assert!(trace.iter().any(|line| line.contains("cond_node_linked")));
    assert!(trace.iter().any(|line| line.contains("cond_before_wait")));
    assert!(
        trace
            .iter()
            .any(|line| line.contains("cond_after_mutex_unlock"))
    );
    assert!(
        trace
            .iter()
            .any(|line| line.contains("cond_barrier_handover"))
    );
}

#[test]
fn one_hundred_seeds_two_thousand_steps() {
    let _serial = support::serial();
    let mut total = 0u64;
    for seed in 0..100 {
        reset();
        let kernel = HostKernel::new(KernelCaps::full(), seed);
        init_main(&kernel, 0x1000_0000, 0x1_0000);
        let shared: &'static Shared = Box::leak(Box::new(Shared {
            mutex: Mutex::new(),
            cond: Cond::new(),
            waiters: 4,
            rounds: 50,
        }));
        let threads = start_waiters(&kernel, shared);
        for round in 0..shared.rounds {
            let mut spins = 0;
            while shared.cond.waiter_count() < 4 && spins < 100_000 {
                kernel.yield_now();
                spins += 1;
            }
            raven_libc_thread::cond::broadcast(&kernel, &shared.cond).unwrap();
            let target = (round + 1) * 4;
            let mut spins = 0;
            while ROUNDS_DONE.load(Ordering::Acquire) < target && spins < 100_000 {
                kernel.yield_now();
                spins += 1;
            }
        }
        for thread in threads {
            let _ = raven_libc_thread::thread::join::join(&kernel, thread);
        }
        total = total.wrapping_add(kernel.sched().steps());
        assert!(!kernel.sched().deadlocked(), "seed {seed}");
    }
    eprintln!("scheduler: {total} steps over 100 seeds");
    assert!(total > 2_000, "total steps {total}");
}
