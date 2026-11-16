// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `tzname` data symbol.

use core::ffi::c_char;
use core::ptr::null_mut;
use core::sync::atomic::AtomicPtr;

/// `tzname`: the standard and daylight abbreviations.
#[unsafe(no_mangle)]
pub static tzname: [AtomicPtr<c_char>; 2] =
    [AtomicPtr::new(null_mut()), AtomicPtr::new(null_mut())];
