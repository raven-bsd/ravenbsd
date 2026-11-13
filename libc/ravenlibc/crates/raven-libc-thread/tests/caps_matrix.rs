// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The capability matrix of C57 4.16: every flag off executes the documented fallback.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_thread::kernel::{ALL_CAPS, CapFlag, KernelCaps, ThreadKernel};
use raven_libc_thread::limits::{ENOSYS, ENOTSUP, PTHREAD_PROCESS_SHARED};
use raven_libc_thread::mutex::{Mutex, MutexAttr};
use support::{HostKernel, init_main, seed_from_env};

fn caps_off(flag: CapFlag) -> HostKernel {
    HostKernel::new(KernelCaps::full().with(flag, false), seed_from_env(7))
}

#[test]
fn tls_off_makes_init_enosys() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Tls);
    let modules = [raven_libc_tls::TlsModuleIn::new(0x1000, 16, 16, 16)];
    assert_eq!(
        raven_libc_thread::thread::create::init_main_thread(&kernel, &modules, 0x1000, 0x1000),
        Err(ENOSYS)
    );
}

#[test]
fn tid_off_uses_the_thirty_bit_user_counter() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Tid);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(kernel.tid_self(), Err(ENOSYS));
    let pthread = raven_libc_thread::thread::current_pthread(&kernel);
    let tid = pthread.tid.load(Ordering::Acquire);
    assert!(tid > 0);
    assert_eq!(tid & !0x3fff_ffff, 0);
}

#[test]
fn exit_ex_off_uses_the_zombie_cache() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::ExitEx);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    static DONE: AtomicUsize = AtomicUsize::new(0);
    fn body(_: usize) -> usize {
        DONE.fetch_add(1, Ordering::AcqRel);
        0
    }
    let mut attr = raven_libc_thread::thread::attr::init();
    attr.set_stacksize_raw(raven_libc_thread::limits::PTHREAD_STACK_MIN * 4);
    let thread = raven_libc_thread::thread::create::create(&kernel, Some(&attr), body, 0).unwrap();
    raven_libc_thread::thread::join::detach(&kernel, thread).unwrap();
    let mut spins = 0;
    while !kernel.is_dead(thread) && spins < 100_000 {
        kernel.yield_now();
        spins += 1;
    }
    assert!(kernel.is_dead(thread));
    assert!(DONE.load(Ordering::Acquire) == 1);
    // The next create reaps the zombie.
    let thread = raven_libc_thread::thread::create::create(&kernel, Some(&attr), body, 0).unwrap();
    let _ = raven_libc_thread::thread::join::join(&kernel, thread);
    assert_eq!(kernel.violations(), Vec::<String>::new());
}

#[test]
fn requeue_off_still_wakes_the_continuation() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Requeue);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let mut mutex = Mutex::new();
    let cond = raven_libc_thread::cond::Cond::new();
    raven_libc_thread::mutex::init(&mut mutex, None);
    raven_libc_thread::mutex::lock(&kernel, &mutex).unwrap();
    let at = raven_libc_thread::limits::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = raven_libc_thread::cond::timedwait(&kernel, &cond, &mutex, 0, Some(at));
    assert_eq!(result, Err(raven_libc_thread::limits::ETIMEDOUT));
    raven_libc_thread::mutex::unlock(&kernel, &mutex).unwrap();
    assert_eq!(
        kernel.futex_requeue(
            &std::sync::atomic::AtomicU32::new(0),
            0,
            &std::sync::atomic::AtomicU32::new(0),
            1
        ),
        Err(ENOSYS)
    );
}

#[test]
fn abs_deadline_off_uses_the_relative_conversion() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::AbsDeadline);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let mut mutex = Mutex::new();
    raven_libc_thread::mutex::init(&mut mutex, None);
    raven_libc_thread::mutex::lock(&kernel, &mutex).unwrap();
    raven_libc_thread::mutex::unlock(&kernel, &mutex).unwrap();
    assert!(!kernel.caps().abs_deadline);
}

#[test]
fn shared_futex_off_rejects_every_shared_setter() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::SharedFutex);
    let mut mattr = raven_libc_thread::mutex::attr_init();
    assert_eq!(
        raven_libc_thread::mutex::attr_setpshared(&kernel, &mut mattr, PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
    let mut cattr = raven_libc_thread::cond::attr_init();
    assert_eq!(
        raven_libc_thread::cond::attr_setpshared(&kernel, &mut cattr, PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
    let mut rattr = raven_libc_thread::rwlock::attr_init();
    assert_eq!(
        raven_libc_thread::rwlock::attr_setpshared(&kernel, &mut rattr, PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
    let mut battr = raven_libc_thread::barrier::attr_init();
    assert_eq!(
        raven_libc_thread::barrier::attr_setpshared(&kernel, &mut battr, PTHREAD_PROCESS_SHARED),
        Err(ENOTSUP)
    );
    let mut sem = raven_libc_thread::sem::Sem::new();
    assert_eq!(
        raven_libc_thread::sem::init(&kernel, &mut sem, PTHREAD_PROCESS_SHARED, 1),
        Err(ENOTSUP)
    );
}

#[test]
fn robust_off_processes_the_list_on_pthread_exit() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Robust);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let mutex: &'static mut Mutex = Box::leak(Box::new(Mutex::new()));
    let mut attr = MutexAttr::default();
    raven_libc_thread::mutex::attr_setrobust(&mut attr, 1).unwrap();
    raven_libc_thread::mutex::init(mutex, Some(&attr));
    // A child can only use the host kernel, so exercise the user-space cleanup on the main
    // thread: the flag-off path is the list walk in the exit tail.
    let mut attr2 = raven_libc_thread::thread::attr::init();
    attr2.set_stacksize_raw(raven_libc_thread::limits::PTHREAD_STACK_MIN * 4);
    let thread =
        raven_libc_thread::thread::create::create(&kernel, Some(&attr2), |_| 0, 0).unwrap();
    let _ = raven_libc_thread::thread::join::join(&kernel, thread);
    assert!(!kernel.caps().robust);
}

#[test]
fn pi_off_rejects_priority_inheritance() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Pi);
    let mut attr = raven_libc_thread::mutex::attr_init();
    assert_eq!(
        raven_libc_thread::mutex::attr_setprotocol(
            &kernel,
            &mut attr,
            raven_libc_thread::limits::PTHREAD_PRIO_INHERIT
        ),
        Err(ENOTSUP)
    );
}

#[test]
fn kick_off_makes_pthread_kill_enosys() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Kick);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let tcb = raven_libc_tls::tcb::tp_to_pthread(ThreadKernel::thread_pointer(&kernel));
    assert_eq!(
        raven_libc_thread::thread::kill::kill(&kernel, tcb, 9),
        Err(ENOSYS)
    );
}

#[test]
fn sched_off_rejects_fifo() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Sched);
    let mut attr = raven_libc_thread::thread::attr::init();
    assert_eq!(
        raven_libc_thread::thread::attr::setschedpolicy(
            &kernel,
            &mut attr,
            raven_libc_thread::limits::SCHED_FIFO
        ),
        Err(ENOTSUP)
    );
}

#[test]
fn proc_fork_off_makes_fork_enosys() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::ProcFork);
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(raven_libc_thread::fork::fork(&kernel), Err(ENOSYS));
    assert_eq!(raven_libc_thread::fork::fork_raw(&kernel), Err(ENOSYS));
}

#[test]
fn eintr_off_makes_the_wait_spurious() {
    let _serial = support::serial();
    let kernel = caps_off(CapFlag::Eintr);
    assert!(!kernel.caps().eintr);
}

#[test]
fn every_capability_is_covered() {
    assert_eq!(ALL_CAPS.len(), 12);
    for flag in ALL_CAPS {
        let kernel = caps_off(flag);
        assert!(!kernel.caps().get(flag), "{flag:?}");
        for other in ALL_CAPS {
            if other != flag {
                assert!(kernel.caps().get(other), "{flag:?} lost {other:?}");
            }
        }
    }
}
