// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_get_priority_max(2)`.
use core::ffi::c_int;

/// The maximum priority of `policy` in the single-policy table.
#[unsafe(no_mangle)]
pub extern "C" fn sched_get_priority_max(policy: c_int) -> c_int {
    let result = crate::ffi::map_proc::sched_bounds(policy).map(|pair| pair.0);
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
