// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `unlockpt` C-ABI placeholder.
//!
//! RavenBSD has no pty scheme yet (PG-54), so the entry point is an explicit `ENOSYS` rather than
//! pretending that a `/dev/ptmx`-style node exists.

use core::ffi::c_int;

use super::unsupported;

/// `unlockpt` (PG-54).
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlockpt(_fd: c_int) -> c_int {
    unsupported()
}
