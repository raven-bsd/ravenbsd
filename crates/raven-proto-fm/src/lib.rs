// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded FMA ereport schema and a no-allocation kernel-to-userland ring.
//!
//! The fixed-width wire format lives in `wire.rs` so the kernel `fm` module can include the same
//! source with `#[path]`. Illumos FMA was read for the contract only (CDDL, no code copied).

#![no_std]
#![forbid(unsafe_code)]

mod wire;

pub use wire::*;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
