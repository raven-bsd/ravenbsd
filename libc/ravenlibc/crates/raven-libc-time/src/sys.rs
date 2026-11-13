// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The platform seam of the time crate.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the fake's test arithmetic is intentional.
//!
//! Every clock read, sleep, file read and `issetugid` query goes through this
//! trait; the pure logic never calls a syscall, reads the environment or opens a
//! file by itself. `raven-libc-platform` implements it for the real process.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Timespec;

/// A clock the crate can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockId {
    /// `CLOCK_REALTIME` and its aliases.
    Realtime,
    /// `CLOCK_MONOTONIC` and its aliases.
    Monotonic,
}

/// Host services needed by the time logic.
pub trait TimeSys {
    /// Read one of the two supported clocks.
    fn clock_get(&self, id: ClockId) -> Result<Timespec, Errno>;
    /// Sleep for a relative number of nanoseconds.
    fn nanosleep(&self, ns: u64) -> Result<(), Errno>;
    /// Open `path`, read once into `buf`, close, and return the bytes read.
    ///
    /// `path` is not NUL-terminated. The implementation may return `E2BIG`
    /// when the file does not fit the caller's buffer.
    fn read_file(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno>;
    /// Whether the process is privileged (`AT_SECURE` / set-id).
    fn issetugid(&self) -> bool;
}

/// Build an [`Errno`] from a Linux number.
#[must_use]
pub const fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

/// A `localtime_r`-shaped callback carried by the formatting contexts.
pub type LocaltimeFn = fn(&crate::tz::TzState, i64) -> Result<crate::tz::Tm, Errno>;

#[cfg(test)]
/// Scripted platform used by the unit and integration tests.
pub mod testing {
    use alloc::vec::Vec;
    use core::cell::{Cell, RefCell};

    use raven_libc_abi::errno::Errno;
    use raven_libc_abi::structs::Timespec;

    use super::{ClockId, TimeSys, errno};

    /// A scripted platform for the pure tests.
    pub struct FakeSys {
        /// Current value returned by both clocks.
        pub clock: Cell<Timespec>,
        /// Scripted results for successive `nanosleep` calls.
        pub sleep_results: RefCell<Vec<Result<(), Errno>>>,
        /// The bytes `read_file` returns.
        pub file: RefCell<Vec<u8>>,
        /// Whether `issetugid` returns true.
        pub setuid: bool,
        /// Number of `nanosleep` calls.
        pub sleeps: Cell<usize>,
        /// Nanoseconds to advance before returning a scripted `EINTR`.
        pub eintr_advance: Cell<u64>,
    }

    impl FakeSys {
        /// A fake at `clock`.
        pub fn new(clock: Timespec) -> Self {
            Self {
                clock: Cell::new(clock),
                sleep_results: RefCell::new(Vec::new()),
                file: RefCell::new(Vec::new()),
                setuid: false,
                sleeps: Cell::new(0),
                eintr_advance: Cell::new(0),
            }
        }

        /// A fake at the epoch.
        pub fn epoch() -> Self {
            Self::new(Timespec::ZERO)
        }

        /// Set the current clock.
        pub fn set_clock(&self, clock: Timespec) {
            self.clock.set(clock);
        }

        /// Add one scripted `nanosleep` result.
        pub fn push_sleep(&self, result: Result<(), Errno>) {
            self.sleep_results.borrow_mut().push(result);
        }

        /// Set the file contents.
        pub fn set_file(&self, bytes: &[u8]) {
            *self.file.borrow_mut() = bytes.to_vec();
        }

        /// Advance the clock by `ns`.
        pub fn advance(&self, ns: u64) {
            let current = self.clock.get();
            let total = (current.tv_sec as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(current.tv_nsec as u64)
                .saturating_add(ns);
            self.clock.set(Timespec {
                tv_sec: (total / 1_000_000_000) as i64,
                tv_nsec: (total % 1_000_000_000) as i64,
            });
        }
    }

    impl TimeSys for FakeSys {
        fn clock_get(&self, _id: ClockId) -> Result<Timespec, Errno> {
            Ok(self.clock.get())
        }

        fn nanosleep(&self, ns: u64) -> Result<(), Errno> {
            self.sleeps.set(self.sleeps.get() + 1);
            let result = self.sleep_results.borrow_mut().pop().unwrap_or(Ok(()));
            if result.is_ok() {
                self.advance(ns);
            } else {
                self.advance(self.eintr_advance.get());
            }
            result
        }

        fn read_file(&self, _path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
            let file = self.file.borrow();
            if file.len() > buf.len() {
                return Err(errno(raven_libc_abi::errno::E2BIG));
            }
            buf.get_mut(..file.len())
                .ok_or_else(|| errno(raven_libc_abi::errno::E2BIG))?
                .copy_from_slice(&file);
            Ok(file.len())
        }

        fn issetugid(&self) -> bool {
            self.setuid
        }
    }
}
