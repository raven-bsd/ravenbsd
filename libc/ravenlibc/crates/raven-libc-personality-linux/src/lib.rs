// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-personality-linux`: the ring-2 Linux/GNU personality.
//!
//! Ring 2 of the API surface is "a facility one libc family has, implemented once over native
//! Raven primitives" (`02-api-surface` section 1). This crate holds the pure logic of the Linux
//! personality: `epoll`, `eventfd`, `timerfd`, `signalfd`, `inotify`, `prctl`, `personality`,
//! GNU `error()`, `sysinfo`, `sendfile`/`copy_file_range`, `getauxval` and the `*64` alias table.
//!
//! The C symbols live in the shell (`raven-libc/src/personality_linux/**`); this crate never
//! touches a pointer. Every kernel or server interaction goes through a narrow trait in
//! [`seams`], so the host tests are deterministic and the shell decides which primitive is real
//! and which is still an `ENOSYS` stub (`PG-7`, `PG-19`, `PG-21`, `PG-22`, `PG-25`, `PG-38`).
//!
//! The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`), and has no clock,
//! environment or filesystem access of its own.
//!
//! Clean-room note: the Linux/mlibc/musl donors named in the ticket are STUDY for shape only;
//! no line, comment or identifier sequence is copied. The constants are read from the headers
//! C38 installs, and the unit tests parse those headers where a value is not obvious.

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

pub mod alias64;
pub mod auxval;
pub mod epoll;
pub mod error;
pub mod eventfd;
pub mod inotify;
pub mod personality;
pub mod prctl;
pub mod sendfile;
pub mod signalfd;
pub mod sysinfo;
pub mod timerfd;

pub mod seams;

#[cfg(test)]
pub mod testutil;

pub use seams::{Errno, EvqCtl, EvqEvent, FdKind, Handle, SigSet};
