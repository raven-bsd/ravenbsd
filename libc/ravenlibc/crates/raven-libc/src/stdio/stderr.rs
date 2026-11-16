// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `stderr` data symbol.

use core::ffi::c_void;
use core::ptr::null_mut;
use core::sync::atomic::AtomicPtr;

/// `stderr` (a `FILE *` data symbol).
#[unsafe(no_mangle)]
pub static stderr: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
