// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `__stack_chk_guard` data symbol of the C ABI.
//!
//! C50's `raven-libc-ssp` owns the canary logic (`compute_guard`, the failure sequence and the
//! `*_chk` cores); the shell owns the storage the compiler reads. `__raven_libc_start` step 8
//! writes the word once from `AT_RANDOM` and never calls an entropy syscall (`R-17`).
//!
//! The host test build keeps the same storage for the unit test but does not export the C name;
//! the target archive symbol is checked by `tests/archive_exports.rs`.

/// The process stack-protector canary word (`__stack_chk_guard`).
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub static mut __stack_chk_guard: usize = 0;

/// The host-test copy of the guard; it is not a C symbol on the host.
#[cfg(not(target_os = "none"))]
#[allow(non_upper_case_globals)] // the C name of the symbol
static mut __stack_chk_guard: usize = 0;

/// Store the canary computed by the start path.
#[inline(never)] // keep the store in this member so the start object has no data reference
pub(crate) fn set(value: usize) {
    // SAFETY: `__raven_libc_start` calls this once, before any other thread exists, and every
    // other access is a compiler-inserted read of the same word.
    unsafe {
        core::ptr::addr_of_mut!(__stack_chk_guard).write(value);
    }
}

/// The stored canary (host unit test only).
#[cfg(test)]
pub(crate) fn get() -> usize {
    // SAFETY: the test reads the word it just wrote on the same thread.
    unsafe { core::ptr::addr_of!(__stack_chk_guard).read() }
}

#[cfg(test)]
#[path = "__stack_chk_guard_ut.rs"]
mod tests;
