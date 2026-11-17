// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! POSIX IPC and terminal-attribute policy for ravenlibc (P1c-ipc-tty).
//!
//! The kernel exposes no AIO, POSIX message-queue, STREAMS or SysV IPC primitive yet. This crate
//! owns the parts that do not need such a primitive: the `termios` speed accessors, the `ftok`
//! derivation, name/attribute validation and the typed "not supported" answer. It never fakes a
//! queue: every entry point that needs a kernel object reports [`consts::ENOSYS`] (or `EBADF`
//! for a descriptor that can never name a queue) and the C-ABI shell turns that into `-1` with
//! `errno` set.
//!
//! The crate is `no_std`, allocation-free and `forbid(unsafe_code)`; the C-ABI shell in
//! `raven-libc/src/compat/**` owns the pointers and the `errno` writes.

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

pub mod aio;
pub mod consts;
pub mod mqueue;
pub mod stropts;
pub mod sysv;
pub mod termios;

pub use consts::{EBADF, EFAULT, EINVAL, ENAMETOOLONG, ENOSYS, NAME_MAX};
