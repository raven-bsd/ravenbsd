// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic bounds of the pledge and unveil models.

/// Maximum accepted pledge string length.
pub const MAX_PLEDGE_STRING: usize = 4_096;
/// Maximum number of unveil entries.
pub const MAX_UNVEIL: usize = 128;
/// Number of promises in the OpenBSD table.
pub const PROMISE_COUNT: usize = 35;
