// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheduling glue (T8).

pub mod nice;

pub use nice::{NZERO, clamp_priority, nice};

#[cfg(test)]
#[path = "sched_ut.rs"]
mod tests;
