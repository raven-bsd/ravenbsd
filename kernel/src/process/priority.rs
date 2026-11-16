// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX nice values and scheduler policy constants.
//!
//! The process keeps the exact nice value (-20..19); the scheduler has 32 levels (0 highest,
//! 31 lowest). The table maps nice 0 to `PRIORITY_DEFAULT` (16), so a process that never calls
//! `setpriority` keeps its existing placement.
//!
//! `SCHED_FIFO`/`SCHED_RR` are not implemented. The syscall surface returns `ENOTSUP` for them
//! rather than pretending the fair scheduler honours a real-time policy.

/// Lowest POSIX nice value (highest priority).
pub const NICE_MIN: i32 = -20;
/// Highest POSIX nice value (lowest priority).
pub const NICE_MAX: i32 = 19;

/// `PRIO_PROCESS`.
pub const PRIO_PROCESS: u32 = 0;
/// `PRIO_GROUP`.
pub const PRIO_GROUP: u32 = 1;
/// `PRIO_USER`.
pub const PRIO_USER: u32 = 2;

/// The only policy the fair scheduler honours today.
pub const SCHED_OTHER: u32 = 0;
/// Real-time FIFO (not implemented; `ENOTSUP`).
pub const SCHED_FIFO: u32 = 1;
/// Real-time round-robin (not implemented; `ENOTSUP`).
pub const SCHED_RR: u32 = 2;

/// One run-queue level per nice value, indexed by `nice - NICE_MIN`.
const NICE_TO_PRIORITY: [u8; 40] = [
    0, 0, 1, 2, 3, 4, 4, 5, 6, 7, 8, 8, 9, 10, 11, 12, 12, 13, 14, 15, 16, 16, 17, 18, 19, 19, 20,
    21, 22, 23, 23, 24, 25, 26, 27, 27, 28, 29, 30, 31,
];

/// Clamp a requested nice value to the POSIX range.
pub fn clamp_nice(nice: i32) -> i32 {
    nice.clamp(NICE_MIN, NICE_MAX)
}

/// Map a POSIX nice value to the scheduler's 0..31 run-queue level.
pub fn nice_to_priority(nice: i32) -> u8 {
    let index = usize::try_from(nice.saturating_sub(NICE_MIN)).unwrap_or(0);
    NICE_TO_PRIORITY
        .get(index)
        .copied()
        .unwrap_or(crate::sched::thread::PRIORITY_DEFAULT)
}
