// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Entropy for ravenlibc.
//!
//! `getrandom` fills from the kernel's boot-seeded pool through the native `GETRANDOM` syscall
//! (PG-52) and falls back to the `rand:` scheme only when the kernel cannot serve it.
//! `getentropy` still reads `rand:`. The `ENTROPY_WEAK` flag stays set (H-5) and is queryable by
//! the hardening area.

pub mod getentropy;

pub use getentropy::{
    GETENTROPY_MAX, cached_handle, entropy_weak, getentropy, getrandom, mark_strong, read_rand,
    seed,
};

#[cfg(test)]
#[path = "random_ut.rs"]
mod tests;
