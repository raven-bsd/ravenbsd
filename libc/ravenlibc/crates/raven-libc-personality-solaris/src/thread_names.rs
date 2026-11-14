// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The Solaris-named thread and synchronisation spellings over [`PosixThreads`].
//!
//! CLEANROOM: `06-solaris-design.md` A-sol-5 (threads are always on; no `libpthread`) and
//! B-sol-11 (the `thr_*` API is a second *spelling*, never a second implementation). Every
//! function here is at most two calls into [`PosixThreads`]; the translations are exactly the
//! table of C67 5.C and are the whole difference from POSIX.
//!
//! All functions return the error number (0 on success) and never touch `errno`; that is the
//! convention of the POSIX calls they wrap (C67 5.A).

use raven_libc_abi::limits::SEM_VALUE_MAX;

use crate::consts::{
    ENOTSUP, ETIME, PTHREAD_STACK_MIN, THR_BOUND, THR_DAEMON, THR_DETACHED, THR_NEW_LWP,
    THR_SUSPENDED, USYNC_PROCESS, USYNC_THREAD,
};
use crate::seams::{
    CondT, Errno, KeyDtor, MutexT, PosixThreads, RwlockT, SemaT, SigSet, StackPlan, StartFn,
    ThreadAttrPlan, Timespec,
};

/// Every `THR_*` bit this ticket understands.
pub const THR_KNOWN_FLAGS: i64 =
    THR_BOUND | THR_NEW_LWP | THR_DETACHED | THR_SUSPENDED | THR_DAEMON;

/// An error number as the C `int` return.
fn code(error: Errno) -> i32 {
    i32::from(error.0)
}

/// `Ok(())` as 0, `Err(e)` as the error number.
fn result(result: Result<(), Errno>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => code(error),
    }
}

/// Map `USYNC_THREAD`/`USYNC_PROCESS` to the `pshared` attribute.
///
/// # Errors
/// `EINVAL` for any other value (C67 5.A).
pub fn usync_to_pshared(kind: i32) -> Result<bool, Errno> {
    match kind {
        USYNC_THREAD => Ok(false),
        USYNC_PROCESS => Ok(true),
        _ => Err(Errno::EINVAL),
    }
}

/// Translate `thr_create` flags and stack arguments to an attribute plan.
///
/// `THR_BOUND`, `THR_NEW_LWP` and `THR_DAEMON` are accepted and ignored; `THR_DETACHED` is
/// honoured; `THR_SUSPENDED` is `ENOTSUP` until `PG-48`; an unknown bit is `EINVAL`.
///
/// # Errors
/// `EINVAL` for an unknown bit or a stack size below `PTHREAD_STACK_MIN`; `ENOTSUP` for
/// `THR_SUSPENDED` (C67 5.A).
pub fn thr_flags_to_plan(
    flags: i64,
    stack_base: usize,
    stack_size: usize,
) -> Result<ThreadAttrPlan, Errno> {
    if flags & !THR_KNOWN_FLAGS != 0 {
        return Err(Errno::EINVAL);
    }
    if flags & THR_SUSPENDED != 0 {
        return Err(ENOTSUP);
    }
    let detached = flags & THR_DETACHED != 0;
    let stack = if stack_base != 0 {
        // A base without a size cannot describe a usable stack (RD-new).
        if stack_size < PTHREAD_STACK_MIN {
            return Err(Errno::EINVAL);
        }
        StackPlan::SetStack {
            base: stack_base,
            size: stack_size,
        }
    } else if stack_size != 0 {
        if stack_size < PTHREAD_STACK_MIN {
            return Err(Errno::EINVAL);
        }
        StackPlan::SetSize(stack_size)
    } else {
        StackPlan::Default
    };
    Ok(ThreadAttrPlan { detached, stack })
}

/// `thr_create(stack_base, stack_size, start, arg, flags, *new_thread)`.
///
/// # Errors
/// The attribute errors of [`thr_flags_to_plan`] and the `pthread_create` error.
pub fn thr_create<P: PosixThreads>(
    threads: &mut P,
    stack_base: usize,
    stack_size: usize,
    start: StartFn,
    arg: usize,
    flags: i64,
    new_thread: Option<&mut usize>,
) -> i32 {
    let plan = match thr_flags_to_plan(flags, stack_base, stack_size) {
        Ok(plan) => plan,
        Err(error) => return code(error),
    };
    match threads.create(&plan, start, arg) {
        Ok(identifier) => {
            if let Some(slot) = new_thread {
                *slot = identifier;
            }
            0
        }
        Err(error) => code(error),
    }
}

/// `thr_exit(status)` = `pthread_exit`.
pub fn thr_exit<P: PosixThreads>(threads: &mut P, status: usize) -> ! {
    threads.exit(status)
}

/// `thr_join(wait_for, *departed, *status)` = `pthread_join`.
///
/// `wait_for == 0` ("any thread") is `ENOTSUP` until a thread registry exists (C67 12.2).
pub fn thr_join<P: PosixThreads>(
    threads: &mut P,
    wait_for: usize,
    departed: Option<&mut usize>,
    status: Option<&mut usize>,
) -> i32 {
    if wait_for == 0 {
        return code(ENOTSUP);
    }
    match threads.join(wait_for) {
        Ok(value) => {
            if let Some(slot) = departed {
                *slot = wait_for;
            }
            if let Some(slot) = status {
                *slot = value;
            }
            0
        }
        Err(error) => code(error),
    }
}

/// `thr_self()` = `pthread_self`.
pub fn thr_self<P: PosixThreads>(threads: &P) -> usize {
    threads.self_()
}

/// `thr_yield()` = `sched_yield`.
pub fn thr_yield<P: PosixThreads>(threads: &mut P) {
    threads.yield_();
}

/// `thr_kill(thread, sig)` = `pthread_kill`.
pub fn thr_kill<P: PosixThreads>(threads: &mut P, thread: usize, sig: i32) -> i32 {
    result(threads.kill(thread, sig))
}

/// `thr_sigsetmask(how, set, oset)` = `pthread_sigmask`.
pub fn thr_sigsetmask<P: PosixThreads>(
    threads: &mut P,
    how: i32,
    set: Option<&SigSet>,
    old: Option<&mut SigSet>,
) -> i32 {
    use raven_libc_abi::signal::{SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK};
    if !matches!(how, SIG_BLOCK | SIG_UNBLOCK | SIG_SETMASK) {
        return code(Errno::EINVAL);
    }
    result(threads.sigmask(how, set, old))
}

/// `thr_keycreate(*key, dtor)` = `pthread_key_create`.
pub fn thr_keycreate<P: PosixThreads>(
    threads: &mut P,
    dtor: Option<KeyDtor>,
    key: &mut u32,
) -> i32 {
    match threads.key_create(dtor) {
        Ok(created) => {
            *key = created;
            0
        }
        Err(error) => code(error),
    }
}

/// `thr_setspecific(key, value)` = `pthread_setspecific`.
pub fn thr_setspecific<P: PosixThreads>(threads: &mut P, key: u32, value: usize) -> i32 {
    result(threads.setspecific(key, value))
}

/// `thr_getspecific(key, *value)` = `pthread_getspecific`.
pub fn thr_getspecific<P: PosixThreads>(threads: &mut P, key: u32, value: &mut usize) -> i32 {
    match threads.getspecific(key) {
        Ok(found) => {
            *value = found;
            0
        }
        Err(error) => code(error),
    }
}

/// `thr_min_stack()`: `PTHREAD_STACK_MIN` (16384, `donor-map/14` O-abi-13).
pub const fn thr_min_stack() -> usize {
    PTHREAD_STACK_MIN
}

/// `thr_main()`: 1 on the initial thread, 0 elsewhere.
pub fn thr_main<P: PosixThreads>(threads: &P) -> i32 {
    if threads.is_initial() { 1 } else { 0 }
}

/// `thr_getprio(thread, *priority)` = the priority half of `pthread_getschedparam`.
pub fn thr_getprio<P: PosixThreads>(threads: &mut P, thread: usize, priority: &mut i32) -> i32 {
    match threads.getprio(thread) {
        Ok(value) => {
            *priority = value;
            0
        }
        Err(error) => code(error),
    }
}

/// `thr_setprio(thread, priority)`: only priority 0 is supported (`TBD-11`).
pub fn thr_setprio<P: PosixThreads>(threads: &mut P, thread: usize, priority: i32) -> i32 {
    if priority != 0 {
        return code(ENOTSUP);
    }
    result(threads.setprio(thread, priority))
}

/// `thr_setconcurrency(n)`: store the process-wide hint; negative is `EINVAL`.
pub fn thr_setconcurrency(hint: &mut i32, level: i32) -> i32 {
    if level < 0 {
        return code(Errno::EINVAL);
    }
    *hint = level;
    0
}

/// `thr_getconcurrency()`: the process-wide hint.
pub fn thr_getconcurrency(hint: &i32) -> i32 {
    *hint
}

/// `thr_suspend(thread)`: `ENOSYS` until `PG-48`.
pub fn thr_suspend<P: PosixThreads>(_threads: &mut P, _thread: usize) -> i32 {
    code(Errno::ENOSYS)
}

/// `thr_continue(thread)`: `ENOSYS` until `PG-48`.
pub fn thr_continue<P: PosixThreads>(_threads: &mut P, _thread: usize) -> i32 {
    code(Errno::ENOSYS)
}

/// `mutex_init(m, type, arg)` = `pthread_mutex_init` with a `pshared` attribute.
pub fn mutex_init<P: PosixThreads>(
    threads: &mut P,
    mutex: &mut MutexT,
    kind: i32,
    arg_is_null: bool,
) -> i32 {
    let pshared = match usync_to_pshared(kind) {
        Ok(value) => value,
        Err(error) => return code(error),
    };
    if !arg_is_null {
        return code(Errno::EINVAL);
    }
    result(threads.mutex_init(mutex, pshared))
}

/// `mutex_destroy(m)`.
pub fn mutex_destroy<P: PosixThreads>(threads: &mut P, mutex: &mut MutexT) -> i32 {
    result(threads.mutex_destroy(mutex))
}

/// `mutex_lock(m)`.
pub fn mutex_lock<P: PosixThreads>(threads: &mut P, mutex: &mut MutexT) -> i32 {
    result(threads.mutex_lock(mutex))
}

/// `mutex_trylock(m)`; `EBUSY` passes through.
pub fn mutex_trylock<P: PosixThreads>(threads: &mut P, mutex: &mut MutexT) -> i32 {
    result(threads.mutex_trylock(mutex))
}

/// `mutex_unlock(m)`.
pub fn mutex_unlock<P: PosixThreads>(threads: &mut P, mutex: &mut MutexT) -> i32 {
    result(threads.mutex_unlock(mutex))
}

/// `cond_init(c, type, arg)` = `pthread_cond_init` with a `pshared` attribute.
pub fn cond_init<P: PosixThreads>(
    threads: &mut P,
    cond: &mut CondT,
    kind: i32,
    arg_is_null: bool,
) -> i32 {
    let pshared = match usync_to_pshared(kind) {
        Ok(value) => value,
        Err(error) => return code(error),
    };
    if !arg_is_null {
        return code(Errno::EINVAL);
    }
    result(threads.cond_init(cond, pshared))
}

/// `cond_destroy(c)`.
pub fn cond_destroy<P: PosixThreads>(threads: &mut P, cond: &mut CondT) -> i32 {
    result(threads.cond_destroy(cond))
}

/// `cond_wait(c, m)`.
pub fn cond_wait<P: PosixThreads>(threads: &mut P, cond: &mut CondT, mutex: &mut MutexT) -> i32 {
    result(threads.cond_wait(cond, mutex))
}

/// `cond_timedwait(c, m, abstime)`: absolute time; `ETIMEDOUT` becomes `ETIME`.
pub fn cond_timedwait<P: PosixThreads>(
    threads: &mut P,
    cond: &mut CondT,
    mutex: &mut MutexT,
    abstime: &Timespec,
) -> i32 {
    match threads.cond_timedwait(cond, mutex, abstime) {
        Ok(()) => 0,
        Err(error) if error == Errno::ETIMEDOUT => code(ETIME),
        Err(error) => code(error),
    }
}

/// `cond_signal(c)`.
pub fn cond_signal<P: PosixThreads>(threads: &mut P, cond: &mut CondT) -> i32 {
    result(threads.cond_signal(cond))
}

/// `cond_broadcast(c)`.
pub fn cond_broadcast<P: PosixThreads>(threads: &mut P, cond: &mut CondT) -> i32 {
    result(threads.cond_broadcast(cond))
}

/// `rwlock_init(rw, type, arg)` = `pthread_rwlock_init` with a `pshared` attribute.
pub fn rwlock_init<P: PosixThreads>(
    threads: &mut P,
    lock: &mut RwlockT,
    kind: i32,
    arg_is_null: bool,
) -> i32 {
    let pshared = match usync_to_pshared(kind) {
        Ok(value) => value,
        Err(error) => return code(error),
    };
    if !arg_is_null {
        return code(Errno::EINVAL);
    }
    result(threads.rwlock_init(lock, pshared))
}

/// `rwlock_destroy(rw)`.
pub fn rwlock_destroy<P: PosixThreads>(threads: &mut P, lock: &mut RwlockT) -> i32 {
    result(threads.rwlock_destroy(lock))
}

/// `rw_rdlock(rw)`.
pub fn rw_rdlock<P: PosixThreads>(threads: &mut P, lock: &mut RwlockT) -> i32 {
    result(threads.rwlock_rdlock(lock))
}

/// `rw_wrlock(rw)`.
pub fn rw_wrlock<P: PosixThreads>(threads: &mut P, lock: &mut RwlockT) -> i32 {
    result(threads.rwlock_wrlock(lock))
}

/// `rw_tryrdlock(rw)`; `EBUSY` passes through.
pub fn rw_tryrdlock<P: PosixThreads>(threads: &mut P, lock: &mut RwlockT) -> i32 {
    result(threads.rwlock_tryrdlock(lock))
}

/// `rw_trywrlock(rw)`; `EBUSY` passes through.
pub fn rw_trywrlock<P: PosixThreads>(threads: &mut P, lock: &mut RwlockT) -> i32 {
    result(threads.rwlock_trywrlock(lock))
}

/// `rw_unlock(rw)`.
pub fn rw_unlock<P: PosixThreads>(threads: &mut P, lock: &mut RwlockT) -> i32 {
    result(threads.rwlock_unlock(lock))
}

/// `sema_init(s, count, type, arg)` = `sem_init(sem, pshared, count)`.
pub fn sema_init<P: PosixThreads>(
    threads: &mut P,
    sema: &mut SemaT,
    count: u32,
    kind: i32,
    arg_is_null: bool,
) -> i32 {
    let pshared = match usync_to_pshared(kind) {
        Ok(value) => value,
        Err(error) => return code(error),
    };
    if !arg_is_null {
        return code(Errno::EINVAL);
    }
    let maximum = u32::try_from(SEM_VALUE_MAX).unwrap_or(0);
    if count > maximum {
        return code(Errno::EINVAL);
    }
    result(threads.sem_init(sema, pshared, count))
}

/// `sema_destroy(s)`.
pub fn sema_destroy<P: PosixThreads>(threads: &mut P, sema: &mut SemaT) -> i32 {
    result(threads.sem_destroy(sema))
}

/// `sema_wait(s)`; `EINTR` is returned, not retried.
pub fn sema_wait<P: PosixThreads>(threads: &mut P, sema: &mut SemaT) -> i32 {
    result(threads.sem_wait(sema))
}

/// `sema_trywait(s)`; `EAGAIN` becomes `EBUSY`.
pub fn sema_trywait<P: PosixThreads>(threads: &mut P, sema: &mut SemaT) -> i32 {
    match threads.sem_trywait(sema) {
        Ok(()) => 0,
        Err(error) if error == Errno::EAGAIN => code(Errno::EBUSY),
        Err(error) => code(error),
    }
}

/// `sema_post(s)`.
pub fn sema_post<P: PosixThreads>(threads: &mut P, sema: &mut SemaT) -> i32 {
    result(threads.sem_post(sema))
}

#[cfg(test)]
#[path = "thread_names_ut.rs"]
mod tests;
