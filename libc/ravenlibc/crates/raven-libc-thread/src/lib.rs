// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `raven-libc-thread`: pthreads, C11 threads, TSD, cancellation and the internal locks.
//!
//! Layer L2b. Everything is generic over [`ThreadKernel`]: production
//! instantiates [`SysKernel`](raw::SysKernel) once, the host tests instantiate
//! `HostKernel` once, and no other instantiation is created (R-10). The crate never allocates:
//! thread stacks, TSD arrays and TLS blocks come from one `map` per thread, the atfork table is a
//! fixed array and the key table is a fixed array (A-thr-8).
//!
//! The crate does not depend on `raven-libc-signal` (RD-new 1). Signal masks are TCB words and
//! are updated with atomics; the public `pthread_sigmask` belongs to C58.
//!
//! `unsafe` is confined to [`raw`]: the thread entry asm, the TLS descriptor asm, the atomic
//! views of user-supplied lock words, the robust list and the production kernel.

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]
// Address arithmetic, lock congestion counts and word indices are bounded by the wrapper sizes and
// the capability checks; keeping the donor's arithmetic visible is more auditable than rewriting
// every expression with wrapping/checked methods (same rationale as `raven-libc-malloc`).
#![allow(clippy::arithmetic_side_effects)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

#[cfg(test)]
extern crate alloc;
#[cfg(test)]
extern crate self as raven_libc_thread;

#[cfg(test)]
#[path = "test_support_ut.rs"]
mod test_support;

pub mod barrier;
pub mod c11;
pub mod cond;
pub mod fork;
pub mod kernel;
pub mod limits;
pub mod mutex;
#[allow(unsafe_code)] // the one audited module: asm, word views, robust list and SysKernel
pub mod raw;
pub mod rwlock;
pub mod sem;
pub mod spin;
pub mod sync;
pub mod thread;
pub mod tls_access;
pub mod tlsdesc;

pub use kernel::{CreateArgs, KernelCaps, ThreadKernel};
pub use limits::{Errno, Timespec};

/// The process-wide thread state (the musl `libc` thread fields, no allocation).
pub(crate) mod libc_state;

/// Read-only (and fork-child) probes of the process state, for the host tests and the shell.
#[doc(hidden)]
pub mod state_probe {
    /// The current thread count.
    #[must_use]
    pub fn threads() -> usize {
        crate::libc_state::LIBC.threads()
    }

    /// `need_locks`.
    #[must_use]
    pub fn need_locks() -> i32 {
        crate::libc_state::LIBC.need_locks()
    }

    /// Force the thread count (fork-child tests).
    pub fn set_threads(count: usize) {
        crate::libc_state::LIBC.set_threads(count);
    }

    /// Force `need_locks` (tests).
    pub fn set_need_locks(value: i32) {
        crate::libc_state::LIBC.set_need_locks(value);
    }
}

/// A scheduler hand-over point. For [`SysKernel`](raw::SysKernel) this is a no-op; the host fake
/// records the point and may switch threads.
#[macro_export]
macro_rules! sched_point {
    ($kernel:expr, $name:literal) => {{
        $kernel.sched_point($name);
    }};
}

/// `true` when the crate is built for the host test profile.
pub const HOST_TESTS: bool = cfg!(test);
