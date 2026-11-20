// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getdtablesize(3)`.

use core::ffi::c_int;

pub(crate) fn from_soft(soft: u32) -> c_int {
    i32::try_from(soft).unwrap_or(i32::MAX)
}

/// The soft `RLIMIT_NOFILE` value.
#[unsafe(no_mangle)]
pub extern "C" fn getdtablesize() -> c_int {
    from_soft(crate::ffi::platform::global().rlimits().nofile_soft)
}

#[cfg(test)]
#[path = "getdtablesize_ut.rs"]
mod tests;
