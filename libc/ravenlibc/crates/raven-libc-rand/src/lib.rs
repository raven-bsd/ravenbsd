// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc `arc4random` family (area 15 3.A.1).
//!
//! The crate is `no_std`, allocation-free and `forbid(unsafe_code)`. It does not own the lock,
//! the `mmap` state page or the `getentropy` syscall; those belong to C55 and the shell. It does
//! own the state machine, the uniform rejection loop, the fork wipe and the weak-entropy gate.

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
#![allow(clippy::arithmetic_side_effects)] // The port keeps the C size_t arithmetic.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod arc4random;
pub mod chacha;
pub mod fork;
pub mod getentropy;
pub mod platform;
pub mod uniform;
pub mod weak;

pub use arc4random::{Arc4Random, BLOCKSZ, IVSZ, KEYSZ, REKEY_BASE, RSBUFSZ};
pub use chacha::ChaCha;
pub use fork::ForkEvents;
pub use getentropy::{
    GETENTROPY_MAX, GRND_INSECURE, GRND_NONBLOCK, GRND_RANDOM, GrndFlags, RandError,
    getentropy_check, getrandom_flags, getrandom_len,
};
#[cfg(test)]
pub use platform::NoEntropy;
pub use platform::RandPlatform;
pub use uniform::uniform_with;
pub use weak::{ENTROPY_WEAK_BUILD, entropy_is_weak, mark_entropy_weak};
