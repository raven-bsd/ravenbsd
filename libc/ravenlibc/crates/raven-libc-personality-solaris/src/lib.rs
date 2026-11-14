// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-personality-solaris`: the ring-2 Solaris/illumos personality.
//!
//! Ring 2 of the API surface is "a facility one libc family has, implemented once over native
//! Raven primitives" (`02-api-surface` section 1). This crate holds the pure logic of the
//! Solaris personality: event ports (`port_*`), `getexecname`, `getisax`, and the Solaris-named
//! thread and synchronisation wrappers (`thr_*`, `mutex_*`, `cond_*`, `rwlock_*`/`rw_*`,
//! `sema_*`) over the POSIX threads seam.
//!
//! The crate is designed from the prose of `architecture/raven-libc/06-solaris-design.md`
//! (A-sol-4, A-sol-5, A-sol-47, A-sol-49, A-sol-63) and is CLEANROOM: no illumos line, comment
//! or identifier is reproduced. The C symbols live in the shell
//! (`raven-libc/src/personality_solaris/**`); this crate never touches a pointer. Every kernel
//! interaction goes through a narrow trait in [`seams`], so the host tests are deterministic
//! and the shell decides which primitive is real and which is still `ENOSYS`.
//!
//! `raven-libc-thread` (C57) is merged: [`seams::PosixThreads`] is implemented by the recording
//! fake in the tests and by the shell over the real crate. The remaining kernel gaps are
//! returned by the C57 seams (`PG-2` thread create, `PG-4` shared futex, `PG-7` kick), never
//! hidden by a fake success.
//!
//! The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`), and has no clock,
//! environment or filesystem access of its own.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
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
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)]

extern crate alloc;

pub mod consts;
pub mod identity;
pub mod port;
pub mod seams;
pub mod thread_names;

#[cfg(test)]
pub mod testutil;

pub use seams::{Errno, Handle};
