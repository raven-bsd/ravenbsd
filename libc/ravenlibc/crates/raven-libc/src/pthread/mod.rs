// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The pthread shell family (C64); this pass binds the main-thread TLS initialiser.

#[path = "_pthread_cleanup_pop.rs"]
mod _pthread_cleanup_pop;
#[path = "_pthread_cleanup_push.rs"]
mod _pthread_cleanup_push;
#[path = "common.rs"]
mod common;
#[path = "__raven_init_main_thread.rs"]
mod init_main_thread;
#[path = "pthread_atfork.rs"]
mod pthread_atfork;
#[path = "pthread_attr_destroy.rs"]
mod pthread_attr_destroy;
#[path = "pthread_attr_getdetachstate.rs"]
mod pthread_attr_getdetachstate;
#[path = "pthread_attr_getguardsize.rs"]
mod pthread_attr_getguardsize;
#[path = "pthread_attr_getinheritsched.rs"]
mod pthread_attr_getinheritsched;
#[path = "pthread_attr_getschedparam.rs"]
mod pthread_attr_getschedparam;
#[path = "pthread_attr_getschedpolicy.rs"]
mod pthread_attr_getschedpolicy;
#[path = "pthread_attr_getscope.rs"]
mod pthread_attr_getscope;
#[path = "pthread_attr_getstack.rs"]
mod pthread_attr_getstack;
#[path = "pthread_attr_getstacksize.rs"]
mod pthread_attr_getstacksize;
#[path = "pthread_attr_init.rs"]
mod pthread_attr_init;
#[path = "pthread_attr_setdetachstate.rs"]
mod pthread_attr_setdetachstate;
#[path = "pthread_attr_setguardsize.rs"]
mod pthread_attr_setguardsize;
#[path = "pthread_attr_setinheritsched.rs"]
mod pthread_attr_setinheritsched;
#[path = "pthread_attr_setschedparam.rs"]
mod pthread_attr_setschedparam;
#[path = "pthread_attr_setschedpolicy.rs"]
mod pthread_attr_setschedpolicy;
#[path = "pthread_attr_setscope.rs"]
mod pthread_attr_setscope;
#[path = "pthread_attr_setstack.rs"]
mod pthread_attr_setstack;
#[path = "pthread_attr_setstacksize.rs"]
mod pthread_attr_setstacksize;
#[path = "pthread_barrier_destroy.rs"]
mod pthread_barrier_destroy;
#[path = "pthread_barrier_init.rs"]
mod pthread_barrier_init;
#[path = "pthread_barrier_wait.rs"]
mod pthread_barrier_wait;
#[path = "pthread_barrierattr_destroy.rs"]
mod pthread_barrierattr_destroy;
#[path = "pthread_barrierattr_getpshared.rs"]
mod pthread_barrierattr_getpshared;
#[path = "pthread_barrierattr_init.rs"]
mod pthread_barrierattr_init;
#[path = "pthread_barrierattr_setpshared.rs"]
mod pthread_barrierattr_setpshared;
#[path = "pthread_cancel.rs"]
mod pthread_cancel;
#[path = "pthread_cond_broadcast.rs"]
mod pthread_cond_broadcast;
#[path = "pthread_cond_clockwait.rs"]
mod pthread_cond_clockwait;
#[path = "pthread_cond_destroy.rs"]
mod pthread_cond_destroy;
#[path = "pthread_cond_init.rs"]
mod pthread_cond_init;
#[path = "pthread_cond_signal.rs"]
mod pthread_cond_signal;
#[path = "pthread_cond_timedwait.rs"]
mod pthread_cond_timedwait;
#[path = "pthread_cond_wait.rs"]
mod pthread_cond_wait;
#[path = "pthread_condattr_destroy.rs"]
mod pthread_condattr_destroy;
#[path = "pthread_condattr_getclock.rs"]
mod pthread_condattr_getclock;
#[path = "pthread_condattr_getpshared.rs"]
mod pthread_condattr_getpshared;
#[path = "pthread_condattr_init.rs"]
mod pthread_condattr_init;
#[path = "pthread_condattr_setclock.rs"]
mod pthread_condattr_setclock;
#[path = "pthread_condattr_setpshared.rs"]
mod pthread_condattr_setpshared;
#[path = "pthread_create.rs"]
mod pthread_create;
#[path = "pthread_detach.rs"]
mod pthread_detach;
#[path = "pthread_equal.rs"]
mod pthread_equal;
#[path = "pthread_exit.rs"]
mod pthread_exit;
#[path = "pthread_getaffinity_np.rs"]
mod pthread_getaffinity_np;
#[path = "pthread_getattr_default_np.rs"]
mod pthread_getattr_default_np;
#[path = "pthread_getattr_np.rs"]
mod pthread_getattr_np;
#[path = "pthread_getconcurrency.rs"]
mod pthread_getconcurrency;
#[path = "pthread_getcpuclockid.rs"]
mod pthread_getcpuclockid;
#[path = "pthread_getname_np.rs"]
mod pthread_getname_np;
#[path = "pthread_getschedparam.rs"]
mod pthread_getschedparam;
#[path = "pthread_getspecific.rs"]
mod pthread_getspecific;
#[path = "pthread_join.rs"]
mod pthread_join;
#[path = "pthread_key_create.rs"]
mod pthread_key_create;
#[path = "pthread_key_delete.rs"]
mod pthread_key_delete;
#[path = "pthread_kill.rs"]
mod pthread_kill;
#[path = "pthread_mutex_clocklock.rs"]
mod pthread_mutex_clocklock;
#[path = "pthread_mutex_consistent.rs"]
mod pthread_mutex_consistent;
#[path = "pthread_mutex_destroy.rs"]
mod pthread_mutex_destroy;
#[path = "pthread_mutex_getprioceiling.rs"]
mod pthread_mutex_getprioceiling;
#[path = "pthread_mutex_init.rs"]
mod pthread_mutex_init;
#[path = "pthread_mutex_lock.rs"]
mod pthread_mutex_lock;
#[path = "pthread_mutex_setprioceiling.rs"]
mod pthread_mutex_setprioceiling;
#[path = "pthread_mutex_timedlock.rs"]
mod pthread_mutex_timedlock;
#[path = "pthread_mutex_trylock.rs"]
mod pthread_mutex_trylock;
#[path = "pthread_mutex_unlock.rs"]
mod pthread_mutex_unlock;
#[path = "pthread_mutexattr_destroy.rs"]
mod pthread_mutexattr_destroy;
#[path = "pthread_mutexattr_getprioceiling.rs"]
mod pthread_mutexattr_getprioceiling;
#[path = "pthread_mutexattr_getprotocol.rs"]
mod pthread_mutexattr_getprotocol;
#[path = "pthread_mutexattr_getpshared.rs"]
mod pthread_mutexattr_getpshared;
#[path = "pthread_mutexattr_getrobust.rs"]
mod pthread_mutexattr_getrobust;
#[path = "pthread_mutexattr_gettype.rs"]
mod pthread_mutexattr_gettype;
#[path = "pthread_mutexattr_init.rs"]
mod pthread_mutexattr_init;
#[path = "pthread_mutexattr_setprioceiling.rs"]
mod pthread_mutexattr_setprioceiling;
#[path = "pthread_mutexattr_setprotocol.rs"]
mod pthread_mutexattr_setprotocol;
#[path = "pthread_mutexattr_setpshared.rs"]
mod pthread_mutexattr_setpshared;
#[path = "pthread_mutexattr_setrobust.rs"]
mod pthread_mutexattr_setrobust;
#[path = "pthread_mutexattr_settype.rs"]
mod pthread_mutexattr_settype;
#[path = "pthread_once.rs"]
mod pthread_once;
#[path = "pthread_rwlock_clockrdlock.rs"]
mod pthread_rwlock_clockrdlock;
#[path = "pthread_rwlock_clockwrlock.rs"]
mod pthread_rwlock_clockwrlock;
#[path = "pthread_rwlock_destroy.rs"]
mod pthread_rwlock_destroy;
#[path = "pthread_rwlock_init.rs"]
mod pthread_rwlock_init;
#[path = "pthread_rwlock_rdlock.rs"]
mod pthread_rwlock_rdlock;
#[path = "pthread_rwlock_timedrdlock.rs"]
mod pthread_rwlock_timedrdlock;
#[path = "pthread_rwlock_timedwrlock.rs"]
mod pthread_rwlock_timedwrlock;
#[path = "pthread_rwlock_tryrdlock.rs"]
mod pthread_rwlock_tryrdlock;
#[path = "pthread_rwlock_trywrlock.rs"]
mod pthread_rwlock_trywrlock;
#[path = "pthread_rwlock_unlock.rs"]
mod pthread_rwlock_unlock;
#[path = "pthread_rwlock_wrlock.rs"]
mod pthread_rwlock_wrlock;
#[path = "pthread_rwlockattr_destroy.rs"]
mod pthread_rwlockattr_destroy;
#[path = "pthread_rwlockattr_getpshared.rs"]
mod pthread_rwlockattr_getpshared;
#[path = "pthread_rwlockattr_init.rs"]
mod pthread_rwlockattr_init;
#[path = "pthread_rwlockattr_setpshared.rs"]
mod pthread_rwlockattr_setpshared;
#[path = "pthread_self.rs"]
mod pthread_self;
#[path = "pthread_setaffinity_np.rs"]
mod pthread_setaffinity_np;
#[path = "pthread_setattr_default_np.rs"]
mod pthread_setattr_default_np;
#[path = "pthread_setcancelstate.rs"]
mod pthread_setcancelstate;
#[path = "pthread_setcanceltype.rs"]
mod pthread_setcanceltype;
#[path = "pthread_setconcurrency.rs"]
mod pthread_setconcurrency;
#[path = "pthread_setname_np.rs"]
mod pthread_setname_np;
#[path = "pthread_setschedparam.rs"]
mod pthread_setschedparam;
#[path = "pthread_setschedprio.rs"]
mod pthread_setschedprio;
#[path = "pthread_setspecific.rs"]
mod pthread_setspecific;
#[path = "pthread_sigmask.rs"]
mod pthread_sigmask;
#[path = "pthread_spin_destroy.rs"]
mod pthread_spin_destroy;
#[path = "pthread_spin_init.rs"]
mod pthread_spin_init;
#[path = "pthread_spin_lock.rs"]
mod pthread_spin_lock;
#[path = "pthread_spin_trylock.rs"]
mod pthread_spin_trylock;
#[path = "pthread_spin_unlock.rs"]
mod pthread_spin_unlock;
#[path = "pthread_testcancel.rs"]
mod pthread_testcancel;
#[path = "pthread_timedjoin_np.rs"]
mod pthread_timedjoin_np;
#[path = "pthread_tryjoin_np.rs"]
mod pthread_tryjoin_np;
#[path = "pthread_yield.rs"]
mod pthread_yield;
#[path = "sem_clockwait.rs"]
mod sem_clockwait;
#[path = "sem_close.rs"]
mod sem_close;
#[path = "sem_destroy.rs"]
mod sem_destroy;
#[path = "sem_getvalue.rs"]
mod sem_getvalue;
#[path = "sem_init.rs"]
mod sem_init;
#[path = "raven_va_sem_open.rs"]
mod sem_open;
#[path = "sem_post.rs"]
mod sem_post;
#[path = "sem_timedwait.rs"]
mod sem_timedwait;
#[path = "sem_trywait.rs"]
mod sem_trywait;
#[path = "sem_unlink.rs"]
mod sem_unlink;
#[path = "sem_wait.rs"]
mod sem_wait;
