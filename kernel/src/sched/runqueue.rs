// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel run-queue aliases over the host-tested `raven-sched-core` data structures.

use alloc::sync::Arc;

pub use raven_sched_core::{CAPACITY, LEVELS, RunQueue, SleepList};

use super::thread::Thread;

/// One priority level is a FIFO of ready threads.
pub type ThreadQueue = RunQueue<Arc<Thread>>;

/// The sleep list holds the thread, its wake token and its deadline.
pub type ThreadSleepList = SleepList<(Arc<Thread>, u64, isize)>;

/// True when `priority` names a run-queue level.
pub fn valid_priority(priority: u8) -> bool {
    usize::from(priority) < LEVELS
}

const _: () = assert!(LEVELS == 32);
const _: () = assert!(CAPACITY >= 16);
