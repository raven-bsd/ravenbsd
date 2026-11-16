// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `timezone` data symbol.

use core::sync::atomic::AtomicI64;

/// `timezone`: the standard offset west of UTC.
#[unsafe(no_mangle)]
pub static timezone: AtomicI64 = AtomicI64::new(0);
