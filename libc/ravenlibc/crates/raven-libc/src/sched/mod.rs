// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The scheduler family of the C-ABI shell (C64 4.2, rows P25/X57): one policy (`SCHED_OTHER`)
//! and one CPU today (O-15-2/O-15-8). `sched_yield` is the `YIELD` system call; the parameter,
//! policy and affinity conversions live in [`crate::ffi::map_proc`]; `sched_setaffinity` reports
//! `ENOSYS` until the affinity primitive exists.

pub(crate) mod sched_get_priority_max;
pub(crate) mod sched_get_priority_min;
pub(crate) mod sched_getaffinity;
pub(crate) mod sched_getcpu;
pub(crate) mod sched_getparam;
pub(crate) mod sched_getscheduler;
pub(crate) mod sched_rr_get_interval;
pub(crate) mod sched_setaffinity;
pub(crate) mod sched_setparam;
pub(crate) mod sched_setscheduler;
pub(crate) mod sched_yield;
