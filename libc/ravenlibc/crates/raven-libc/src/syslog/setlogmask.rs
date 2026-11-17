// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setlogmask` from `<syslog.h>`.

use core::ffi::c_int;
use core::sync::atomic::Ordering;

use super::MASK;

/// `setlogmask`.
#[unsafe(no_mangle)]
pub extern "C" fn setlogmask(mask: c_int) -> c_int {
    if mask == 0 {
        return MASK.load(Ordering::Relaxed);
    }
    MASK.swap(mask, Ordering::Relaxed)
}
