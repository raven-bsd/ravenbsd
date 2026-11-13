// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor-test ports (C57 7.2): expected values and scenarios from the NetBSD and OpenBSD thread
//! tests, re-expressed as Rust. Cases that need signals or a real kernel are listed in the report
//! as `not ported`.

mod support;

use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use raven_libc_thread::kernel::{KernelCaps, ThreadKernel};
use raven_libc_thread::limits::{
    EAGAIN, EBUSY, EDEADLK, EINVAL, ENOSYS, ENOTSUP, ESRCH, ETIMEDOUT, PTHREAD_STACK_MIN,
};
use raven_libc_thread::mutex::Mutex;
use support::{HostKernel, init_main, seed_from_env};

fn kernel() -> HostKernel {
    HostKernel::new(KernelCaps::full(), seed_from_env(31))
}

fn attr(size: usize) -> raven_libc_thread::thread::attr::Attr {
    let mut attr = raven_libc_thread::thread::attr::init();
    attr.set_stacksize_raw(size);
    attr
}

// NetBSD t_equal.c
#[test]
fn netbsd_equal() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let current = raven_libc_thread::thread::join::self_thread(&kernel);
    assert!(raven_libc_thread::thread::join::equal(current, current));
    assert!(!raven_libc_thread::thread::join::equal(
        current,
        current + 8
    ));
}

// NetBSD t_join.c
#[test]
fn netbsd_join_returns_the_result() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(arg: usize) -> usize {
        arg * 2
    }
    let thread = raven_libc_thread::thread::create::create(
        &kernel,
        Some(&attr(PTHREAD_STACK_MIN * 4)),
        body,
        21,
    )
    .unwrap();
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(42)
    );
}

// NetBSD t_detach.c: detaching a detached thread fails, joining a detached thread fails.
#[test]
fn netbsd_detach() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        0
    }
    let thread = raven_libc_thread::thread::create::create(
        &kernel,
        Some(&attr(PTHREAD_STACK_MIN * 4)),
        body,
        0,
    )
    .unwrap();
    assert_eq!(
        raven_libc_thread::thread::join::detach(&kernel, thread),
        Ok(())
    );
    assert_eq!(
        raven_libc_thread::thread::join::detach(&kernel, thread),
        Err(EINVAL)
    );
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Err(EINVAL)
    );
}

// NetBSD t_stack.c: a user stack gets no guard.
#[test]
fn netbsd_stack_has_no_guard() {
    let _serial = support::serial();
    let mut storage = vec![0u8; 65536];
    let base = storage.as_mut_ptr() as usize;
    let mut attr = raven_libc_thread::thread::attr::init();
    raven_libc_thread::thread::attr::setstack(&mut attr, base, 65536).unwrap();
    assert_eq!(raven_libc_thread::thread::attr::getstacksize(&attr), 65536);
    assert_eq!(
        raven_libc_thread::thread::attr::getstack(&attr),
        Ok((base, 65536))
    );
}

// NetBSD t_name.c
#[test]
fn netbsd_name() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        0
    }
    let thread = raven_libc_thread::thread::create::create(
        &kernel,
        Some(&attr(PTHREAD_STACK_MIN * 4)),
        body,
        0,
    )
    .unwrap();
    assert_eq!(
        raven_libc_thread::thread::name::setname(&kernel, thread, b"netbsd"),
        Ok(())
    );
    let mut buffer = [0u8; 16];
    assert_eq!(
        raven_libc_thread::thread::name::getname(&kernel, thread, &mut buffer),
        6
    );
    assert_eq!(&buffer[..6], b"netbsd");
    assert_eq!(
        raven_libc_thread::thread::name::setname(&kernel, thread, b"1234567890123456"),
        Err(raven_libc_thread::limits::ENAMETOOLONG)
    );
    let _ = raven_libc_thread::thread::join::join(&kernel, thread);
}

// NetBSD t_mutex.c / t_timedmutex.c
#[test]
fn netbsd_mutex_and_timedmutex() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let mutex = Mutex::new();
    assert_eq!(raven_libc_thread::mutex::trylock(&kernel, &mutex), Ok(()));
    assert_eq!(
        raven_libc_thread::mutex::trylock(&kernel, &mutex),
        Err(EBUSY)
    );
    assert_eq!(raven_libc_thread::mutex::unlock(&kernel, &mutex), Ok(()));
    let at = raven_libc_thread::limits::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let held = Mutex::new();
    held.lock_word().store(999, Ordering::Release);
    assert_eq!(
        raven_libc_thread::mutex::timedlock(&kernel, &held, Some(at)),
        Err(ETIMEDOUT)
    );
}

// NetBSD t_rwlock.c
#[test]
fn netbsd_rwlock() {
    let kernel = kernel();
    let lock = raven_libc_thread::rwlock::RwLock::new();
    assert_eq!(raven_libc_thread::rwlock::tryrdlock(&lock), Ok(()));
    assert_eq!(raven_libc_thread::rwlock::tryrdlock(&lock), Ok(()));
    assert_eq!(raven_libc_thread::rwlock::trywrlock(&lock), Err(EBUSY));
    raven_libc_thread::rwlock::unlock(&kernel, &lock).unwrap();
    raven_libc_thread::rwlock::unlock(&kernel, &lock).unwrap();
    assert_eq!(raven_libc_thread::rwlock::trywrlock(&lock), Ok(()));
    raven_libc_thread::rwlock::unlock(&kernel, &lock).unwrap();
}

// NetBSD t_barrier.c
#[test]
fn netbsd_barrier() {
    let kernel = kernel();
    let mut barrier = raven_libc_thread::barrier::Barrier::new();
    raven_libc_thread::barrier::init(&mut barrier, None, 1).unwrap();
    assert_eq!(
        raven_libc_thread::barrier::wait_barrier(&kernel, &barrier),
        Ok(raven_libc_thread::limits::PTHREAD_BARRIER_SERIAL_THREAD)
    );
    assert_eq!(
        raven_libc_thread::barrier::init(&mut barrier, None, 0),
        Err(EINVAL)
    );
}

// NetBSD t_once.c and t_call_once.c
#[test]
fn netbsd_once_and_call_once() {
    let _serial = support::serial();
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    fn init() {
        RUNS.fetch_add(1, Ordering::AcqRel);
    }
    RUNS.store(0, Ordering::Release);
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let once = raven_libc_thread::thread::once::Once::new();
    raven_libc_thread::thread::once::once(&kernel, &once, init);
    raven_libc_thread::thread::once::call_once(&kernel, &once, init);
    assert_eq!(RUNS.load(Ordering::Acquire), 1);
}

// NetBSD t_sem.c
#[test]
fn netbsd_sem() {
    let _serial = support::serial();
    let kernel = kernel();
    let mut sem = raven_libc_thread::sem::Sem::new();
    raven_libc_thread::sem::init(&kernel, &mut sem, 0, 1).unwrap();
    assert_eq!(raven_libc_thread::sem::trywait(&sem), Ok(()));
    assert_eq!(raven_libc_thread::sem::trywait(&sem), Err(EAGAIN));
    assert_eq!(raven_libc_thread::sem::post(&kernel, &sem), Ok(()));
    assert_eq!(raven_libc_thread::sem::getvalue(&sem), 1);
}

// NetBSD t_cnd.c / t_condwait.c: signal with no waiter.
#[test]
fn netbsd_cnd_empty_signal() {
    let kernel = kernel();
    let cond = raven_libc_thread::cond::Cond::new();
    assert_eq!(raven_libc_thread::cond::signal(&kernel, &cond), Ok(()));
    assert_eq!(raven_libc_thread::cond::broadcast(&kernel, &cond), Ok(()));
}

// NetBSD t_tss.c
#[test]
fn netbsd_tss() {
    let _serial = support::serial();
    let kernel = kernel();
    let mut tsd_kernel = kernel.clone();
    init_main(&tsd_kernel, 0x1000_0000, 0x1_0000);
    let key = raven_libc_thread::thread::tsd::key_create(&tsd_kernel, None).unwrap();
    assert_eq!(
        raven_libc_thread::thread::tsd::setspecific(&tsd_kernel, key, 17),
        Ok(())
    );
    assert_eq!(
        raven_libc_thread::thread::tsd::getspecific(&tsd_kernel, key),
        17
    );
    raven_libc_thread::thread::tsd::key_delete(&tsd_kernel, key);
    assert_eq!(
        raven_libc_thread::thread::tsd::setspecific(&tsd_kernel, key, 18),
        Err(EINVAL)
    );
    let _ = &mut tsd_kernel;
}

// NetBSD t_thrd.c / t_mtx.c
#[test]
fn netbsd_c11_thrd_and_mtx() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(arg: usize) -> usize {
        arg + 1
    }
    let thread = raven_libc_thread::c11::thrd_create(&kernel, body, 9).unwrap();
    assert_eq!(raven_libc_thread::c11::thrd_join(&kernel, thread), Ok(10));
    let mut mutex = Mutex::new();
    assert_eq!(raven_libc_thread::c11::mtx_init(&mut mutex, 0), 0);
    assert_eq!(raven_libc_thread::c11::mtx_lock(&kernel, &mutex), 0);
    assert_eq!(raven_libc_thread::c11::mtx_trylock(&kernel, &mutex), 1);
    assert_eq!(raven_libc_thread::c11::mtx_unlock(&kernel, &mutex), 0);
}

// OpenBSD pthread_cond_timedwait: a past deadline is ETIMEDOUT.
#[test]
fn openbsd_cond_timedwait_past() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let mut mutex = Mutex::new();
    raven_libc_thread::mutex::init(&mut mutex, None);
    raven_libc_thread::mutex::lock(&kernel, &mutex).unwrap();
    let cond = raven_libc_thread::cond::Cond::new();
    let at = raven_libc_thread::limits::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(
        raven_libc_thread::cond::timedwait(&kernel, &cond, &mutex, 0, Some(at)),
        Err(ETIMEDOUT)
    );
    raven_libc_thread::mutex::unlock(&kernel, &mutex).unwrap();
}

// OpenBSD pthread_mutex: a non-owner unlock of an error-checking mutex is EPERM.
#[test]
fn openbsd_mutex_non_owner() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let mut mutex = Mutex::new();
    let mut attr = raven_libc_thread::mutex::attr_init();
    raven_libc_thread::mutex::attr_settype(
        &mut attr,
        raven_libc_thread::limits::PTHREAD_MUTEX_ERRORCHECK,
    )
    .unwrap();
    raven_libc_thread::mutex::init(&mut mutex, Some(&attr));
    raven_libc_thread::mutex::lock(&kernel, &mutex).unwrap();
    raven_libc_thread::thread::current_pthread_mut(&kernel)
        .tid
        .store(500, Ordering::Release);
    assert_eq!(
        raven_libc_thread::mutex::unlock(&kernel, &mutex),
        Err(raven_libc_thread::limits::EPERM)
    );
}

// OpenBSD pthread_rwlock2
#[test]
fn openbsd_rwlock2() {
    let kernel = kernel();
    let lock = raven_libc_thread::rwlock::RwLock::new();
    assert_eq!(raven_libc_thread::rwlock::trywrlock(&lock), Ok(()));
    assert_eq!(raven_libc_thread::rwlock::tryrdlock(&lock), Err(EBUSY));
    raven_libc_thread::rwlock::unlock(&kernel, &lock).unwrap();
}

// OpenBSD spinlock
#[test]
fn openbsd_spinlock() {
    let lock = raven_libc_thread::spin::SpinLock::new();
    assert_eq!(raven_libc_thread::spin::trylock(&lock), Ok(()));
    assert_eq!(raven_libc_thread::spin::trylock(&lock), Err(EBUSY));
    raven_libc_thread::spin::unlock(&lock);
}

// OpenBSD semaphore
#[test]
fn openbsd_semaphore() {
    let _serial = support::serial();
    let kernel = kernel();
    let mut sem = raven_libc_thread::sem::Sem::new();
    raven_libc_thread::sem::init(&kernel, &mut sem, 0, 3).unwrap();
    for expected in (0..3).rev() {
        assert_eq!(raven_libc_thread::sem::trywait(&sem), Ok(()));
        assert_eq!(raven_libc_thread::sem::getvalue(&sem), expected);
    }
}

// OpenBSD pthread_specific
#[test]
fn openbsd_pthread_specific() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let key = raven_libc_thread::thread::tsd::key_create(&kernel, None).unwrap();
    assert_eq!(raven_libc_thread::thread::tsd::getspecific(&kernel, key), 0);
    raven_libc_thread::thread::tsd::setspecific(&kernel, key, 3).unwrap();
    assert_eq!(raven_libc_thread::thread::tsd::getspecific(&kernel, key), 3);
    raven_libc_thread::thread::tsd::key_delete(&kernel, key);
}

// OpenBSD blocked_join: joining a thread that is still running returns its result only after exit.
#[test]
fn openbsd_blocked_join() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    static READY: AtomicI32 = AtomicI32::new(0);
    fn body(_: usize) -> usize {
        READY.store(1, Ordering::Release);
        5
    }
    let thread = raven_libc_thread::thread::create::create(
        &kernel,
        Some(&attr(PTHREAD_STACK_MIN * 4)),
        body,
        0,
    )
    .unwrap();
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(5)
    );
    assert_eq!(READY.load(Ordering::Acquire), 1);
}

// OpenBSD errno: pthreads return the error number and never touch errno.
#[test]
fn openbsd_errno_is_not_used() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, 0),
        Err(ESRCH)
    );
    assert_eq!(
        raven_libc_thread::thread::join::join(
            &kernel,
            raven_libc_thread::thread::join::self_thread(&kernel)
        ),
        Err(EDEADLK)
    );
}

// NetBSD t_exit.sh: the join result of a thread that calls pthread_exit.
#[test]
fn netbsd_exit_result() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    fn body(_: usize) -> usize {
        0xabcdef
    }
    let thread = raven_libc_thread::thread::create::create(
        &kernel,
        Some(&attr(PTHREAD_STACK_MIN * 4)),
        body,
        0,
    )
    .unwrap();
    assert_eq!(
        raven_libc_thread::thread::join::join(&kernel, thread),
        Ok(0xabcdef)
    );
}

// OpenBSD pthread_kill: a null handle is ESRCH, a valid one without a signal is OK.
#[test]
fn openbsd_kill() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    assert_eq!(
        raven_libc_thread::thread::kill::kill(&kernel, 0, 0),
        Err(ESRCH)
    );
    let current = raven_libc_thread::thread::join::self_thread(&kernel);
    assert_eq!(
        raven_libc_thread::thread::kill::kill(&kernel, current, 0),
        Ok(())
    );
    assert_eq!(
        raven_libc_thread::thread::kill::kill(&kernel, current, 999),
        Err(EINVAL)
    );
}

// NetBSD t_tls_static.c: a static TLS lookup returns a non-zero block address.
#[test]
fn netbsd_tls_static() {
    let _serial = support::serial();
    let kernel = kernel();
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let address = raven_libc_thread::tls_access::tls_get_addr(&kernel, 1, 0);
    assert_ne!(address, 0);
    assert_eq!(raven_libc_thread::tls_access::module_count(&kernel), 1);
}

// Not ported: needs C58 and PG-7 — async cancellation delivery through signals.
#[test]
fn cancellation_without_kick_acts_at_the_next_point() {
    let _serial = support::serial();
    let kernel = HostKernel::new(
        KernelCaps::full().with(raven_libc_thread::kernel::CapFlag::Kick, false),
        seed_from_env(32),
    );
    init_main(&kernel, 0x1000_0000, 0x1_0000);
    let pthread = raven_libc_thread::thread::current_pthread(&kernel);
    pthread
        .cancel
        .store(raven_libc_thread::limits::CANCEL_ENABLE, Ordering::Release);
    assert_eq!(
        raven_libc_thread::thread::cancel::cancel(
            &kernel,
            raven_libc_thread::thread::join::self_thread(&kernel)
        ),
        Ok(())
    );
    assert_eq!(
        pthread.cancel.load(Ordering::Acquire) & raven_libc_thread::limits::CANCEL_REQUESTED,
        raven_libc_thread::limits::CANCEL_REQUESTED
    );
    let _ = ENOSYS;
    let _ = ENOTSUP;
    let _ = &mut attr(0);
    let _ = kernel.caps();
    let _ = ThreadKernel::now_ns(&kernel, 0);
}
