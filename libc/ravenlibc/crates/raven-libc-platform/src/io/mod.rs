// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Grouping for the T2/T3 I/O operations.
//!
//! The implementations are methods of [`crate::Platform`] (`open`, `read`, `write`, `stat`,
//! `mkdir`, ...). This module exists so the crate layout matches `donor-map/11` section 3.13 and
//! so the shell can import one place for the operation types.

pub use crate::platform::Rlimits;
pub use crate::stat::StatSource;

/// The size of the readv/writev bounce buffer: one atomic request up to this, a loop beyond.
pub const BOUNCE_LIMIT: usize = 64 * 1024;
