// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one `errno` cell of the shell.
//!
//! `raven-libc-sys` has no pointer accessor yet (C54 exposes `errno()`/`set_errno()` and the
//! TLS-provider seam, not `ptr()`), and `raven-libc-tls` is not on this branch. The shell
//! therefore owns the process-wide cell that `__errno_location` returns; when C57 installs the
//! per-thread provider this module becomes the single caller of it (RD-new 2).

use core::ffi::c_int;
use core::sync::atomic::{AtomicI32, Ordering};

use raven_libc_abi::errno::Errno;

static CELL: AtomicI32 = AtomicI32::new(0);

/// Set `errno` from a C-library error number.
pub(crate) fn set(e: Errno) {
    CELL.store(i32::from(e.0), Ordering::Relaxed);
}

/// Set `errno` from a raw positive number.
pub(crate) fn set_raw(value: i32) {
    CELL.store(value, Ordering::Relaxed);
}

/// Set `errno` from the platform error type.
pub(crate) fn set_platform(e: raven_libc_platform::Errno) {
    CELL.store(i32::from(e.0), Ordering::Relaxed);
}

/// The current `errno` value.
#[must_use]
pub(crate) fn get() -> i32 {
    CELL.load(Ordering::Relaxed)
}

/// The address of the cell, as `__errno_location` returns it.
#[must_use]
pub(crate) fn ptr() -> *mut c_int {
    CELL.as_ptr().cast::<c_int>()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "errno_ut.rs"]
mod tests;
