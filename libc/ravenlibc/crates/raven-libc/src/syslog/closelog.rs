// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `closelog` from `<syslog.h>`.

use core::sync::atomic::Ordering;

use super::{IDENT, OPTION};

/// `closelog`.
#[unsafe(no_mangle)]
pub extern "C" fn closelog() {
    // SAFETY: single-threaded access.
    let cell = unsafe { &mut *IDENT.0.get() };
    cell.clear();
    OPTION.store(0, Ordering::Relaxed);
}
