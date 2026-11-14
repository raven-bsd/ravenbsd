// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The Solaris personality exports (`personality_solaris`, ticket C67).
//!
//! One file per symbol (C67 5.F); the body converts pointers, calls
//! `raven-libc-personality-solaris` once and applies the POSIX or `errno` return convention.
//! `real.rs` owns the real seam implementations, the port table and the concurrency hint.
//! The module is compiled only when the shell feature `personality-solaris` is on.

pub(crate) mod cond_broadcast;
pub(crate) mod cond_destroy;
pub(crate) mod cond_init;
pub(crate) mod cond_signal;
pub(crate) mod cond_timedwait;
pub(crate) mod cond_wait;
pub(crate) mod getexecname;
pub(crate) mod getisax;
pub(crate) mod mutex_destroy;
pub(crate) mod mutex_init;
pub(crate) mod mutex_lock;
pub(crate) mod mutex_trylock;
pub(crate) mod mutex_unlock;
pub(crate) mod port_alert;
pub(crate) mod port_associate;
pub(crate) mod port_create;
pub(crate) mod port_dissociate;
pub(crate) mod port_get;
pub(crate) mod port_getn;
pub(crate) mod port_send;
pub(crate) mod real;
pub(crate) mod rw_rdlock;
pub(crate) mod rw_tryrdlock;
pub(crate) mod rw_trywrlock;
pub(crate) mod rw_unlock;
pub(crate) mod rw_wrlock;
pub(crate) mod rwlock_destroy;
pub(crate) mod rwlock_init;
pub(crate) mod sema_destroy;
pub(crate) mod sema_init;
pub(crate) mod sema_post;
pub(crate) mod sema_trywait;
pub(crate) mod sema_wait;
pub(crate) mod thr_continue;
pub(crate) mod thr_create;
pub(crate) mod thr_exit;
pub(crate) mod thr_getconcurrency;
pub(crate) mod thr_getprio;
pub(crate) mod thr_getspecific;
pub(crate) mod thr_join;
pub(crate) mod thr_keycreate;
pub(crate) mod thr_kill;
pub(crate) mod thr_main;
pub(crate) mod thr_min_stack;
pub(crate) mod thr_self;
pub(crate) mod thr_setconcurrency;
pub(crate) mod thr_setprio;
pub(crate) mod thr_setspecific;
pub(crate) mod thr_sigsetmask;
pub(crate) mod thr_suspend;
pub(crate) mod thr_yield;
