// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Entropy for ravenlibc.
//!
//! `getrandom` and `getentropy` read the `rand:` scheme until PG-52 adds a kernel CSPRNG. The
//! `ENTROPY_WEAK` flag stays set until then (H-5) and is queryable by the hardening area.

pub mod getentropy;

pub use getentropy::{
    GETENTROPY_MAX, cached_handle, entropy_weak, getentropy, getrandom, mark_strong, read_rand,
    seed,
};

#[cfg(test)]
#[path = "random_ut.rs"]
mod tests;
