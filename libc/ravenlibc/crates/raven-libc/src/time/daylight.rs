// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `daylight` data symbol.

use core::sync::atomic::AtomicI32;

/// `daylight`: whether the zone observes daylight time.
#[unsafe(no_mangle)]
pub static daylight: AtomicI32 = AtomicI32::new(0);
