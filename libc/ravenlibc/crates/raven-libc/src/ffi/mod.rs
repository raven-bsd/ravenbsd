// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The shell's conversion helpers: C pointers to slices, result/errno mapping and buffer
//! copies. Compiled on every target and unit-tested on the host.
//!
//! The export modules are `#[cfg(target_os = "none")]`, so these helpers look unused in a host
//! build of the library; they are used by the target artifact and by the host tests.

#![allow(dead_code)]

pub mod cstr;
pub mod env;
pub mod errno;
pub mod map_fmt;
pub mod map_int;
pub mod out;
pub mod platform;
pub mod ret;

/// Serialises the tests that read and write the one process-wide `errno` cell.
#[cfg(test)]
static TEST_BUSY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
pub(crate) struct TestLock;

#[cfg(test)]
impl TestLock {
    /// Spin until the test lock is free.
    pub(crate) fn acquire() -> Self {
        while TEST_BUSY.swap(true, core::sync::atomic::Ordering::Acquire) {
            core::hint::spin_loop();
        }
        Self
    }
}

#[cfg(test)]
impl Drop for TestLock {
    fn drop(&mut self) {
        TEST_BUSY.store(false, core::sync::atomic::Ordering::Release);
    }
}

/// The panic payload type: named here so `src/panic.rs` never spells `PanicInfo` (test A6).
pub(crate) type PanicPayload<'a> = core::panic::PanicInfo<'a>;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
