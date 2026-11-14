// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-personality-bsd`: the ring-2 BSD personality.
//!
//! The crate holds the pure logic of `kqueue`/`kevent`, the BSD `sysctl` API, `closefrom`,
//! `setproctitle`, `getmntinfo`/`getfsstat`, the `pledge`/`unveil` entry glue, `elf_aux_info`
//! and the `pthread_*_np` alias table. The C symbols live in the shell
//! (`raven-libc/src/personality_bsd/**`); every kernel or server interaction goes through the
//! narrow traits of [`seams`].
//!
//! The crate is `no_std` plus `alloc` and class P (`#![forbid(unsafe_code)]`).

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

pub mod aux;
pub mod closefrom;
pub mod errno_alias;
pub mod getmntinfo;
pub mod kqueue;
pub mod pledge_glue;
pub mod pthread_np;
pub mod seams;
pub mod setproctitle;
pub mod sysctl;

#[cfg(test)]
pub mod testutil;

pub use seams::{Errno, EvqCtl, EvqEvent, FdKind, Handle};
