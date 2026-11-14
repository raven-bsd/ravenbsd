// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Tests for `thread_names.rs` (C67 5.A and 5.C: one test per wrapper and flag rule).

use std::panic::{AssertUnwindSafe, catch_unwind};

use raven_libc_abi::signal::{SIG_BLOCK, SIG_SETMASK};

use crate::consts::{
    ENOTSUP, ETIME, PTHREAD_STACK_MIN, THR_BOUND, THR_DAEMON, THR_DETACHED, THR_NEW_LWP,
    THR_SUSPENDED, USYNC_PROCESS, USYNC_THREAD,
};
use crate::seams::{CondT, Errno, MutexT, RwlockT, SemaT, StackPlan, Timespec};
use crate::testutil::{NoThreads, RecordingThreads, no_start};
use crate::thread_names::*;

fn threads() -> RecordingThreads {
    RecordingThreads::new()
}

#[test]
fn usync_thread_is_private() {
    assert_eq!(usync_to_pshared(USYNC_THREAD), Ok(false));
}

#[test]
fn usync_process_is_shared() {
    assert_eq!(usync_to_pshared(USYNC_PROCESS), Ok(true));
}

#[test]
fn usync_other_is_invalid() {
    assert_eq!(usync_to_pshared(2), Err(Errno::EINVAL));
    assert_eq!(usync_to_pshared(-1), Err(Errno::EINVAL));
}

#[test]
fn known_flags_are_the_five_bits() {
    assert_eq!(THR_KNOWN_FLAGS, 0x1f);
}

#[test]
fn plan_default() {
    let plan = thr_flags_to_plan(0, 0, 0).unwrap();
    assert!(!plan.detached);
    assert_eq!(plan.stack, StackPlan::Default);
}

#[test]
fn plan_detached() {
    let plan = thr_flags_to_plan(THR_DETACHED, 0, 0).unwrap();
    assert!(plan.detached);
}

#[test]
fn plan_bound_is_ignored() {
    let plan = thr_flags_to_plan(THR_BOUND, 0, 0).unwrap();
    assert!(!plan.detached);
    assert_eq!(plan.stack, StackPlan::Default);
}

#[test]
fn plan_new_lwp_is_ignored() {
    let plan = thr_flags_to_plan(THR_NEW_LWP, 0, 0).unwrap();
    assert_eq!(plan.stack, StackPlan::Default);
}

#[test]
fn plan_daemon_is_ignored() {
    let plan = thr_flags_to_plan(THR_DAEMON, 0, 0).unwrap();
    assert_eq!(plan.stack, StackPlan::Default);
}

#[test]
fn plan_suspended_is_enotsup() {
    assert_eq!(thr_flags_to_plan(THR_SUSPENDED, 0, 0), Err(ENOTSUP));
}

#[test]
fn plan_suspended_never_starts_a_thread() {
    let mut t = threads();
    let mut identifier = 0usize;
    assert_eq!(
        thr_create(
            &mut t,
            0,
            0,
            no_start,
            0,
            THR_SUSPENDED,
            Some(&mut identifier)
        ),
        i32::from(ENOTSUP.0)
    );
    assert!(t.calls.is_empty());
}

#[test]
fn plan_unknown_bit_is_einval() {
    assert_eq!(thr_flags_to_plan(0x20, 0, 0), Err(Errno::EINVAL));
    assert_eq!(
        thr_flags_to_plan(THR_DETACHED | 0x100, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn plan_base_and_size() {
    let plan = thr_flags_to_plan(0, 0x1000, PTHREAD_STACK_MIN).unwrap();
    assert_eq!(
        plan.stack,
        StackPlan::SetStack {
            base: 0x1000,
            size: PTHREAD_STACK_MIN
        }
    );
}

#[test]
fn plan_base_without_size_is_einval() {
    assert_eq!(thr_flags_to_plan(0, 0x1000, 0), Err(Errno::EINVAL));
}

#[test]
fn plan_base_with_small_size_is_einval() {
    assert_eq!(
        thr_flags_to_plan(0, 0x1000, PTHREAD_STACK_MIN - 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn plan_size_only() {
    let plan = thr_flags_to_plan(0, 0, 32_768).unwrap();
    assert_eq!(plan.stack, StackPlan::SetSize(32_768));
}

#[test]
fn plan_size_below_minimum_is_einval() {
    assert_eq!(
        thr_flags_to_plan(0, 0, PTHREAD_STACK_MIN - 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn plan_size_at_minimum_is_ok() {
    assert_eq!(
        thr_flags_to_plan(0, 0, PTHREAD_STACK_MIN),
        Ok(crate::seams::ThreadAttrPlan {
            detached: false,
            stack: StackPlan::SetSize(PTHREAD_STACK_MIN)
        })
    );
}

#[test]
fn create_passes_the_plan_and_argument() {
    let mut t = threads();
    let mut identifier = 0usize;
    let status = thr_create(
        &mut t,
        0,
        0,
        no_start,
        77,
        THR_DETACHED | THR_BOUND,
        Some(&mut identifier),
    );
    assert_eq!(status, 0);
    assert_eq!(identifier, 42);
    assert_eq!(t.count("create"), 1);
    assert_eq!(t.created_arg, Some(77));
    assert!(t.created_plan.is_some_and(|plan| plan.detached));
}

#[test]
fn create_allows_a_null_out_pointer() {
    let mut t = threads();
    assert_eq!(thr_create(&mut t, 0, 0, no_start, 1, 0, None), 0);
}

#[test]
fn create_passes_the_seam_error() {
    let mut t = threads();
    t.error = Some(Errno::ENOMEM);
    assert_eq!(
        thr_create(&mut t, 0, 0, no_start, 1, 0, None),
        i32::from(Errno::ENOMEM.0)
    );
}

#[test]
fn exit_calls_the_seam_and_does_not_return() {
    let mut t = threads();
    let outcome = catch_unwind(AssertUnwindSafe(|| thr_exit(&mut t, 5)));
    assert!(outcome.is_err());
    assert_eq!(t.exit_status, Some(5));
    assert_eq!(t.count("exit"), 1);
}

#[test]
fn join_zero_is_enotsup() {
    let mut t = threads();
    assert_eq!(thr_join(&mut t, 0, None, None), i32::from(ENOTSUP.0));
    assert!(t.calls.is_empty());
}

#[test]
fn join_sets_departed_and_status() {
    let mut t = threads();
    let mut departed = 0usize;
    let mut status = 0usize;
    assert_eq!(
        thr_join(&mut t, 42, Some(&mut departed), Some(&mut status)),
        0
    );
    assert_eq!(departed, 42);
    assert_eq!(status, 7);
    assert_eq!(t.count("join"), 1);
}

#[test]
fn join_passes_the_error() {
    let mut t = threads();
    t.error = Some(Errno::ESRCH);
    let mut departed = 99usize;
    assert_eq!(
        thr_join(&mut t, 42, Some(&mut departed), None),
        i32::from(Errno::ESRCH.0)
    );
    assert_eq!(departed, 99);
}

#[test]
fn self_returns_the_pthread_id() {
    let mut t = threads();
    t.self_id = 0x1234;
    assert_eq!(thr_self(&t), 0x1234);
}

#[test]
fn yield_calls_sched_yield() {
    let mut t = threads();
    thr_yield(&mut t);
    assert_eq!(t.yields, 1);
}

#[test]
fn kill_forwards() {
    let mut t = threads();
    assert_eq!(thr_kill(&mut t, 42, 9), 0);
    assert_eq!(t.count("kill"), 1);
}

#[test]
fn kill_passes_the_error() {
    let mut t = threads();
    t.error = Some(Errno::ESRCH);
    assert_eq!(thr_kill(&mut t, 42, 9), i32::from(Errno::ESRCH.0));
}

#[test]
fn sigsetmask_forwards_the_mask() {
    let mut t = threads();
    let mut set = [0u64; 16];
    set[0] = 1;
    assert_eq!(thr_sigsetmask(&mut t, SIG_BLOCK, Some(&set), None), 0);
    assert_eq!(t.mask, Some((SIG_BLOCK, Some(set))));
}

#[test]
fn sigsetmask_rejects_an_unknown_how() {
    let mut t = threads();
    assert_eq!(
        thr_sigsetmask(&mut t, 9, None, None),
        i32::from(Errno::EINVAL.0)
    );
    assert!(t.calls.is_empty());
}

#[test]
fn sigsetmask_error_passes_through() {
    let mut t = threads();
    t.error = Some(Errno::EINVAL);
    assert_eq!(
        thr_sigsetmask(&mut t, SIG_SETMASK, None, None),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn keycreate_writes_the_key() {
    let mut t = threads();
    let mut key = 0u32;
    assert_eq!(thr_keycreate(&mut t, None, &mut key), 0);
    assert_eq!(key, 9);
    assert_eq!(t.count("key_create"), 1);
}

#[test]
fn keycreate_passes_the_error() {
    let mut t = threads();
    t.error = Some(Errno::ENOMEM);
    let mut key = 5u32;
    assert_eq!(
        thr_keycreate(&mut t, None, &mut key),
        i32::from(Errno::ENOMEM.0)
    );
    assert_eq!(key, 5);
}

#[test]
fn setspecific_and_getspecific_roundtrip() {
    let mut t = threads();
    assert_eq!(thr_setspecific(&mut t, 9, 0xbeef), 0);
    let mut value = 0usize;
    assert_eq!(thr_getspecific(&mut t, 9, &mut value), 0);
    assert_eq!(value, 0xbeef);
}

#[test]
fn getspecific_absent_key_is_enoent() {
    let mut t = threads();
    let mut value = 0usize;
    assert_eq!(
        thr_getspecific(&mut t, 3, &mut value),
        i32::from(Errno::ENOENT.0)
    );
    assert_eq!(value, 0);
}

#[test]
fn min_stack_is_the_constant() {
    assert_eq!(thr_min_stack(), PTHREAD_STACK_MIN);
}

#[test]
fn main_is_one_on_the_initial_thread() {
    let t = threads();
    assert_eq!(thr_main(&t), 1);
}

#[test]
fn main_is_zero_on_another_thread() {
    let mut t = threads();
    t.initial = false;
    assert_eq!(thr_main(&t), 0);
}

#[test]
fn getprio_writes_the_priority() {
    let mut t = threads();
    t.priority = 0;
    let mut priority = 9;
    assert_eq!(thr_getprio(&mut t, 42, &mut priority), 0);
    assert_eq!(priority, 0);
    assert_eq!(t.count("getprio"), 1);
}

#[test]
fn getprio_passes_the_error() {
    let mut t = threads();
    t.error = Some(Errno::ESRCH);
    let mut priority = 9;
    assert_eq!(
        thr_getprio(&mut t, 42, &mut priority),
        i32::from(Errno::ESRCH.0)
    );
    assert_eq!(priority, 9);
}

#[test]
fn setprio_zero_forwards() {
    let mut t = threads();
    assert_eq!(thr_setprio(&mut t, 42, 0), 0);
    assert_eq!(t.priorities, [(42, 0)]);
}

#[test]
fn setprio_nonzero_is_enotsup_without_a_call() {
    let mut t = threads();
    assert_eq!(thr_setprio(&mut t, 42, 3), i32::from(ENOTSUP.0));
    assert!(t.calls.is_empty());
}

#[test]
fn setconcurrency_stores_and_getconcurrency_reads() {
    let mut hint = 0i32;
    assert_eq!(thr_setconcurrency(&mut hint, 4), 0);
    assert_eq!(hint, 4);
    assert_eq!(thr_getconcurrency(&hint), 4);
}

#[test]
fn setconcurrency_negative_is_einval() {
    let mut hint = 3i32;
    assert_eq!(
        thr_setconcurrency(&mut hint, -1),
        i32::from(Errno::EINVAL.0)
    );
    assert_eq!(hint, 3);
}

#[test]
fn suspend_is_enosys() {
    let mut t = threads();
    assert_eq!(thr_suspend(&mut t, 42), i32::from(Errno::ENOSYS.0));
    assert!(t.calls.is_empty());
}

#[test]
fn continue_is_enosys() {
    let mut t = threads();
    assert_eq!(thr_continue(&mut t, 42), i32::from(Errno::ENOSYS.0));
    assert!(t.calls.is_empty());
}

#[test]
fn mutex_init_thread_is_private() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_init(&mut t, &mut mutex, USYNC_THREAD, true), 0);
    assert_eq!(t.mutex_pshared, [false]);
}

#[test]
fn mutex_init_process_is_shared() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_init(&mut t, &mut mutex, USYNC_PROCESS, true), 0);
    assert_eq!(t.mutex_pshared, [true]);
}

#[test]
fn mutex_init_rejects_an_unknown_type() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        mutex_init(&mut t, &mut mutex, 7, true),
        i32::from(Errno::EINVAL.0)
    );
    assert!(t.calls.is_empty());
}

#[test]
fn mutex_init_rejects_a_nonnull_arg() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        mutex_init(&mut t, &mut mutex, USYNC_THREAD, false),
        i32::from(Errno::EINVAL.0)
    );
    assert!(t.calls.is_empty());
}

#[test]
fn mutex_init_passes_the_error() {
    let mut t = threads();
    t.error = Some(Errno::ENOMEM);
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        mutex_init(&mut t, &mut mutex, USYNC_THREAD, true),
        i32::from(Errno::ENOMEM.0)
    );
}

#[test]
fn mutex_destroy_forwards() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_destroy(&mut t, &mut mutex), 0);
    assert_eq!(t.count("mutex_destroy"), 1);
}

#[test]
fn mutex_lock_forwards() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_lock(&mut t, &mut mutex), 0);
    assert_eq!(t.count("mutex_lock"), 1);
}

#[test]
fn mutex_lock_error_is_a_positive_number() {
    let mut t = threads();
    t.error = Some(Errno::EINVAL);
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_lock(&mut t, &mut mutex), 22);
}

#[test]
fn mutex_trylock_forwards() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_trylock(&mut t, &mut mutex), 0);
    assert_eq!(t.count("mutex_trylock"), 1);
}

#[test]
fn mutex_trylock_ebusy_passes_through() {
    let mut t = threads();
    t.trylock_error = Some(Errno::EBUSY);
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_trylock(&mut t, &mut mutex), i32::from(Errno::EBUSY.0));
}

#[test]
fn mutex_unlock_forwards() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_unlock(&mut t, &mut mutex), 0);
    assert_eq!(t.count("mutex_unlock"), 1);
}

#[test]
fn cond_init_thread_is_private() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(cond_init(&mut t, &mut cond, USYNC_THREAD, true), 0);
    assert_eq!(t.cond_pshared, [false]);
}

#[test]
fn cond_init_process_is_shared() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(cond_init(&mut t, &mut cond, USYNC_PROCESS, true), 0);
    assert_eq!(t.cond_pshared, [true]);
}

#[test]
fn cond_init_rejects_an_unknown_type() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(
        cond_init(&mut t, &mut cond, 3, true),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn cond_init_rejects_a_nonnull_arg() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(
        cond_init(&mut t, &mut cond, USYNC_THREAD, false),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn cond_wait_forwards() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    let mut mutex = MutexT::ZERO;
    assert_eq!(cond_wait(&mut t, &mut cond, &mut mutex), 0);
    assert_eq!(t.count("cond_wait"), 1);
}

#[test]
fn cond_timedwait_forwards() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    let mut mutex = MutexT::ZERO;
    let deadline = Timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    assert_eq!(cond_timedwait(&mut t, &mut cond, &mut mutex, &deadline), 0);
    assert_eq!(t.count("cond_timedwait"), 1);
}

#[test]
fn cond_timedwait_maps_etimedout_to_etime() {
    let mut t = threads();
    t.timedwait_error = Some(Errno::ETIMEDOUT);
    let mut cond = CondT::ZERO;
    let mut mutex = MutexT::ZERO;
    let deadline = Timespec::ZERO;
    assert_eq!(
        cond_timedwait(&mut t, &mut cond, &mut mutex, &deadline),
        i32::from(ETIME.0)
    );
}

#[test]
fn cond_timedwait_passes_etime_through() {
    let mut t = threads();
    t.timedwait_error = Some(ETIME);
    let mut cond = CondT::ZERO;
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        cond_timedwait(&mut t, &mut cond, &mut mutex, &Timespec::ZERO),
        i32::from(ETIME.0)
    );
}

#[test]
fn cond_timedwait_passes_enosys_through() {
    let mut t = threads();
    t.timedwait_error = Some(Errno::ENOSYS);
    let mut cond = CondT::ZERO;
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        cond_timedwait(&mut t, &mut cond, &mut mutex, &Timespec::ZERO),
        i32::from(Errno::ENOSYS.0)
    );
}

#[test]
fn cond_signal_forwards() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(cond_signal(&mut t, &mut cond), 0);
    assert_eq!(t.count("cond_signal"), 1);
}

#[test]
fn cond_broadcast_forwards() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(cond_broadcast(&mut t, &mut cond), 0);
    assert_eq!(t.count("cond_broadcast"), 1);
}

#[test]
fn cond_destroy_forwards() {
    let mut t = threads();
    let mut cond = CondT::ZERO;
    assert_eq!(cond_destroy(&mut t, &mut cond), 0);
    assert_eq!(t.count("cond_destroy"), 1);
}

#[test]
fn rwlock_init_thread_is_private() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(rwlock_init(&mut t, &mut lock, USYNC_THREAD, true), 0);
    assert_eq!(t.rwlock_pshared, [false]);
}

#[test]
fn rwlock_init_process_is_shared() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(rwlock_init(&mut t, &mut lock, USYNC_PROCESS, true), 0);
    assert_eq!(t.rwlock_pshared, [true]);
}

#[test]
fn rwlock_init_rejects_an_unknown_type() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(
        rwlock_init(&mut t, &mut lock, 8, true),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn rwlock_init_rejects_a_nonnull_arg() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(
        rwlock_init(&mut t, &mut lock, USYNC_THREAD, false),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn rwlock_destroy_forwards() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(rwlock_destroy(&mut t, &mut lock), 0);
}

#[test]
fn rw_rdlock_forwards() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(rw_rdlock(&mut t, &mut lock), 0);
    assert_eq!(t.count("rwlock_rdlock"), 1);
}

#[test]
fn rw_wrlock_forwards() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(rw_wrlock(&mut t, &mut lock), 0);
    assert_eq!(t.count("rwlock_wrlock"), 1);
}

#[test]
fn rw_tryrdlock_ebusy_passes_through() {
    let mut t = threads();
    t.tryrdlock_error = Some(Errno::EBUSY);
    let mut lock = RwlockT::ZERO;
    assert_eq!(rw_tryrdlock(&mut t, &mut lock), i32::from(Errno::EBUSY.0));
}

#[test]
fn rw_trywrlock_ebusy_passes_through() {
    let mut t = threads();
    t.trywrlock_error = Some(Errno::EBUSY);
    let mut lock = RwlockT::ZERO;
    assert_eq!(rw_trywrlock(&mut t, &mut lock), i32::from(Errno::EBUSY.0));
}

#[test]
fn rw_unlock_forwards() {
    let mut t = threads();
    let mut lock = RwlockT::ZERO;
    assert_eq!(rw_unlock(&mut t, &mut lock), 0);
    assert_eq!(t.count("rwlock_unlock"), 1);
}

#[test]
fn sema_init_thread_is_private_and_takes_the_count() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_init(&mut t, &mut sema, 3, USYNC_THREAD, true), 0);
    assert_eq!(t.semaphores, [(false, 3)]);
}

#[test]
fn sema_init_process_is_shared() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_init(&mut t, &mut sema, 1, USYNC_PROCESS, true), 0);
    assert_eq!(t.semaphores, [(true, 1)]);
}

#[test]
fn sema_init_rejects_an_unknown_type() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(
        sema_init(&mut t, &mut sema, 1, 2, true),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn sema_init_rejects_a_nonnull_arg() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(
        sema_init(&mut t, &mut sema, 1, USYNC_THREAD, false),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn sema_init_accepts_sem_value_max() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(
        sema_init(&mut t, &mut sema, 0x7fff_ffff, USYNC_THREAD, true),
        0
    );
}

#[test]
fn sema_init_rejects_above_sem_value_max() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(
        sema_init(&mut t, &mut sema, 0x8000_0000, USYNC_THREAD, true),
        i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn sema_destroy_forwards() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_destroy(&mut t, &mut sema), 0);
    assert_eq!(t.count("sem_destroy"), 1);
}

#[test]
fn sema_wait_forwards() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_wait(&mut t, &mut sema), 0);
    assert_eq!(t.count("sem_wait"), 1);
}

#[test]
fn sema_wait_returns_eintr_without_retrying() {
    let mut t = threads();
    t.error = Some(Errno::EINTR);
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_wait(&mut t, &mut sema), i32::from(Errno::EINTR.0));
    assert_eq!(t.count("sem_wait"), 1);
}

#[test]
fn sema_trywait_forwards() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_trywait(&mut t, &mut sema), 0);
    assert_eq!(t.count("sem_trywait"), 1);
}

#[test]
fn sema_trywait_eagain_becomes_ebusy() {
    let mut t = threads();
    t.sem_trywait_error = Some(Errno::EAGAIN);
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_trywait(&mut t, &mut sema), i32::from(Errno::EBUSY.0));
}

#[test]
fn sema_trywait_passes_other_errors_through() {
    let mut t = threads();
    t.error = Some(Errno::EINVAL);
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_trywait(&mut t, &mut sema), i32::from(Errno::EINVAL.0));
}

#[test]
fn sema_post_forwards() {
    let mut t = threads();
    let mut sema = SemaT::ZERO;
    assert_eq!(sema_post(&mut t, &mut sema), 0);
    assert_eq!(t.count("sem_post"), 1);
}

#[test]
fn no_threads_create_is_enosys() {
    let mut t = NoThreads;
    assert_eq!(
        thr_create(&mut t, 0, 0, no_start, 0, 0, None),
        i32::from(Errno::ENOSYS.0)
    );
}

#[test]
fn no_threads_mutex_is_enosys() {
    let mut t = NoThreads;
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        mutex_init(&mut t, &mut mutex, USYNC_THREAD, true),
        i32::from(Errno::ENOSYS.0)
    );
    assert_eq!(mutex_lock(&mut t, &mut mutex), i32::from(Errno::ENOSYS.0));
}

#[test]
fn no_threads_cond_is_enosys() {
    let mut t = NoThreads;
    let mut cond = CondT::ZERO;
    let mut mutex = MutexT::ZERO;
    assert_eq!(
        cond_wait(&mut t, &mut cond, &mut mutex),
        i32::from(Errno::ENOSYS.0)
    );
    assert_eq!(
        cond_timedwait(&mut t, &mut cond, &mut mutex, &Timespec::ZERO),
        i32::from(Errno::ENOSYS.0)
    );
}

#[test]
fn no_threads_rwlock_is_enosys() {
    let mut t = NoThreads;
    let mut lock = RwlockT::ZERO;
    assert_eq!(
        rwlock_init(&mut t, &mut lock, USYNC_THREAD, true),
        i32::from(Errno::ENOSYS.0)
    );
    assert_eq!(rw_rdlock(&mut t, &mut lock), i32::from(Errno::ENOSYS.0));
}

#[test]
fn no_threads_sema_is_enosys() {
    let mut t = NoThreads;
    let mut sema = SemaT::ZERO;
    assert_eq!(
        sema_init(&mut t, &mut sema, 1, USYNC_THREAD, true),
        i32::from(Errno::ENOSYS.0)
    );
    assert_eq!(sema_post(&mut t, &mut sema), i32::from(Errno::ENOSYS.0));
}

#[test]
fn no_threads_keys_are_enosys() {
    let mut t = NoThreads;
    let mut key = 0u32;
    assert_eq!(
        thr_keycreate(&mut t, None, &mut key),
        i32::from(Errno::ENOSYS.0)
    );
}

#[test]
fn no_threads_identity_is_the_initial_thread() {
    let t = NoThreads;
    assert_eq!(thr_self(&t), 0);
    assert_eq!(thr_main(&t), 1);
}

#[test]
fn no_threads_join_is_enosys() {
    let mut t = NoThreads;
    assert_eq!(thr_join(&mut t, 42, None, None), i32::from(Errno::ENOSYS.0));
}

#[test]
fn no_threads_suspend_stays_enosys() {
    let mut t = NoThreads;
    assert_eq!(thr_suspend(&mut t, 1), i32::from(Errno::ENOSYS.0));
}

#[test]
fn wrappers_are_stateless_between_calls() {
    let mut t = threads();
    let mut mutex = MutexT::ZERO;
    assert_eq!(mutex_lock(&mut t, &mut mutex), 0);
    assert_eq!(mutex_lock(&mut t, &mut mutex), 0);
    assert_eq!(t.count("mutex_lock"), 2);
}

#[test]
fn error_numbers_are_never_negative() {
    let mut t = threads();
    t.error = Some(Errno::EACCES);
    let mut mutex = MutexT::ZERO;
    let value = mutex_lock(&mut t, &mut mutex);
    assert!(value > 0);
    assert_eq!(value, 13);
}
