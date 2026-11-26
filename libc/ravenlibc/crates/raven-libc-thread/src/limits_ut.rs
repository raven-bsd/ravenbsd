// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn abort_error_values_match_the_native_abi() {
    assert_eq!(i32::from(EINVAL.0), raven_libc_abi::errno::EINVAL);
    assert_eq!(i32::from(EAGAIN.0), raven_libc_abi::errno::EAGAIN);
    assert_eq!(i32::from(EBUSY.0), raven_libc_abi::errno::EBUSY);
    assert_eq!(i32::from(EDEADLK.0), raven_libc_abi::errno::EDEADLK);
    assert_eq!(i32::from(EPERM.0), raven_libc_abi::errno::EPERM);
    assert_eq!(i32::from(ESRCH.0), raven_libc_abi::errno::ESRCH);
    assert_eq!(i32::from(EINTR.0), raven_libc_abi::errno::EINTR);
    assert_eq!(i32::from(ETIMEDOUT.0), raven_libc_abi::errno::ETIMEDOUT);
    assert_eq!(i32::from(ENOTSUP.0), raven_libc_abi::errno::ENOTSUP);
    assert_eq!(i32::from(ENOSYS.0), raven_libc_abi::errno::ENOSYS);
    assert_eq!(i32::from(EOWNERDEAD.0), raven_libc_abi::errno::EOWNERDEAD);
    assert_eq!(
        i32::from(ENOTRECOVERABLE.0),
        raven_libc_abi::errno::ENOTRECOVERABLE
    );
    assert_eq!(i32::from(ECANCELED.0), raven_libc_abi::errno::ECANCELED);
    assert_eq!(i32::from(EOVERFLOW.0), raven_libc_abi::errno::EOVERFLOW);
    assert_eq!(i32::from(ERANGE.0), raven_libc_abi::errno::ERANGE);
    assert_eq!(i32::from(ENOMEM.0), raven_libc_abi::errno::ENOMEM);
    assert_eq!(i32::from(ENOEXEC.0), raven_libc_abi::errno::ENOEXEC);
}

#[test]
fn keys_max_is_the_abi_value() {
    assert_eq!(PTHREAD_KEYS_MAX, 128);
    assert_eq!(
        PTHREAD_KEYS_MAX,
        raven_libc_abi::limits::PTHREAD_KEYS_MAX as usize
    );
}

#[test]
fn destructor_rounds_are_four() {
    assert_eq!(PTHREAD_DESTRUCTOR_ITERATIONS, 4);
}

#[test]
fn stack_defaults_are_the_open_items() {
    assert_eq!(PTHREAD_STACK_MIN, 16384);
    assert_eq!(DEFAULT_STACK_SIZE, 8 * 1024 * 1024);
    assert_eq!(DEFAULT_GUARD_SIZE, 4096);
    assert!(core::hint::black_box(DEFAULT_STACK_MAX) >= DEFAULT_STACK_SIZE);
    assert!(core::hint::black_box(DEFAULT_GUARD_MAX) >= DEFAULT_GUARD_SIZE);
}

#[test]
fn semaphore_and_barrier_limits() {
    assert_eq!(SEM_VALUE_MAX, 0x7fff_ffff);
    assert_eq!(PTHREAD_BARRIER_SERIAL_THREAD, -1);
    assert_eq!(PTHREAD_CANCELED, usize::MAX);
    assert_eq!(ATFORK_MAX, 128);
    assert_eq!(STACK_CACHE_MAX, 16);
    assert_eq!(REGISTRY_MAX, 32);
}

#[test]
fn mutex_type_bits_are_the_donor_layout() {
    assert_eq!(PTHREAD_MUTEX_NORMAL, 3);
    assert_eq!(PTHREAD_MUTEX_RECURSIVE, 2);
    assert_eq!(PTHREAD_MUTEX_ERRORCHECK, 1);
    assert_eq!(MUTEX_KIND_DEFAULT, 0);
    assert_eq!(MUTEX_KIND_RECURSIVE, 1);
    assert_eq!(MUTEX_KIND_NORMAL, 2);
    assert_eq!(MUTEX_TYPE_ROBUST, 4);
    assert_eq!(MUTEX_TYPE_PI, 8);
    assert_eq!(MUTEX_TYPE_SHARED, 128);
    assert_eq!(MUTEX_TYPE_MASK, 15);
    assert_eq!(PTHREAD_MUTEX_ROBUST, 1);
    assert_eq!(PTHREAD_PROCESS_SHARED, 1);
    assert_eq!(PTHREAD_PROCESS_PRIVATE, 0);
}

#[test]
fn owner_bits_are_the_donor_layout() {
    assert_eq!(OWNER_MASK, 0x3fff_ffff);
    assert_eq!(WAITERS_BIT, 0x8000_0000);
    assert_eq!(OWNER_DIED, 0x4000_0000);
    assert_eq!(MUTEX_UNRECOVERABLE, 0x7fff_ffff);
}

#[test]
fn cancel_bits_are_distinct() {
    let all = CANCEL_ENABLE | CANCEL_ASYNC | CANCEL_REQUESTED | CANCEL_MASKED;
    assert_eq!(all, 0b1111);
    assert_eq!(CANCEL_ENABLE & CANCEL_MASKED, 0);
    assert_eq!(PTHREAD_CANCEL_ENABLE, 0);
    assert_eq!(PTHREAD_CANCEL_DISABLE, 1);
    assert_eq!(PTHREAD_CANCEL_MASKED, 2);
    assert_eq!(PTHREAD_CANCEL_DEFERRED, 0);
    assert_eq!(PTHREAD_CANCEL_ASYNCHRONOUS, 2);
}

#[test]
fn clock_ids() {
    assert_eq!(CLOCK_REALTIME, 0);
    assert_eq!(CLOCK_MONOTONIC, 1);
    assert_eq!(SCHED_OTHER, 0);
}

#[test]
fn timespec_conversion_accepts_a_valid_deadline() {
    let ts = Timespec {
        tv_sec: 2,
        tv_nsec: 3,
    };
    assert_eq!(timespec_to_ns(ts), Some(2_000_000_003));
}

#[test]
fn timespec_conversion_rejects_bad_values() {
    assert_eq!(
        timespec_to_ns(Timespec {
            tv_sec: -1,
            tv_nsec: 0
        }),
        None
    );
    assert_eq!(
        timespec_to_ns(Timespec {
            tv_sec: 0,
            tv_nsec: -1
        }),
        None
    );
    assert_eq!(
        timespec_to_ns(Timespec {
            tv_sec: 0,
            tv_nsec: 1_000_000_000
        }),
        None
    );
    assert_eq!(
        timespec_to_ns(Timespec {
            tv_sec: i64::MAX,
            tv_nsec: 999_999_999
        }),
        None
    );
}

#[test]
fn relative_ns_never_returns_zero() {
    assert_eq!(relative_ns(100, 200), Some(100));
    assert_eq!(relative_ns(100, 100), None);
    assert_eq!(relative_ns(200, 100), None);
    assert_eq!(relative_ns(0, 1), Some(1));
}

#[test]
fn relative_ns_handles_the_wrap_boundary() {
    assert_eq!(relative_ns(u64::MAX - 1, u64::MAX), Some(1));
}

#[test]
fn ids_and_scheduling_constants() {
    assert_eq!(PTHREAD_CREATE_JOINABLE, 0);
    assert_eq!(PTHREAD_CREATE_DETACHED, 1);
    assert_eq!(PTHREAD_SCOPE_SYSTEM, 2);
    assert_eq!(PTHREAD_SCOPE_PROCESS, 0);
    assert_eq!(PTHREAD_PRIO_NONE, 0);
    assert_eq!(PTHREAD_PRIO_INHERIT, 1);
    assert_eq!(PTHREAD_PRIO_PROTECT, 2);
    assert_eq!(SCHED_FIFO, 1);
    assert_eq!(SCHED_RR, 2);
}
