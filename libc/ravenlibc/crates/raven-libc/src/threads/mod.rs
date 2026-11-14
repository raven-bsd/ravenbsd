// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C11 `<threads.h>` shell exports.

#[path = "call_once.rs"]
mod call_once;
#[path = "cnd_broadcast.rs"]
mod cnd_broadcast;
#[path = "cnd_destroy.rs"]
mod cnd_destroy;
#[path = "cnd_init.rs"]
mod cnd_init;
#[path = "cnd_signal.rs"]
mod cnd_signal;
#[path = "cnd_timedwait.rs"]
mod cnd_timedwait;
#[path = "cnd_wait.rs"]
mod cnd_wait;
#[path = "mtx_destroy.rs"]
mod mtx_destroy;
#[path = "mtx_init.rs"]
mod mtx_init;
#[path = "mtx_lock.rs"]
mod mtx_lock;
#[path = "mtx_timedlock.rs"]
mod mtx_timedlock;
#[path = "mtx_trylock.rs"]
mod mtx_trylock;
#[path = "mtx_unlock.rs"]
mod mtx_unlock;
#[path = "thrd_create.rs"]
mod thrd_create;
#[path = "thrd_current.rs"]
mod thrd_current;
#[path = "thrd_detach.rs"]
mod thrd_detach;
#[path = "thrd_equal.rs"]
mod thrd_equal;
#[path = "thrd_exit.rs"]
mod thrd_exit;
#[path = "thrd_join.rs"]
mod thrd_join;
#[path = "thrd_sleep.rs"]
mod thrd_sleep;
#[path = "thrd_yield.rs"]
mod thrd_yield;
#[path = "tss_create.rs"]
mod tss_create;
#[path = "tss_delete.rs"]
mod tss_delete;
#[path = "tss_get.rs"]
mod tss_get;
#[path = "tss_set.rs"]
mod tss_set;
