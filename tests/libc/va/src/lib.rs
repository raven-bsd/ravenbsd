// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host test crate of the ravenlibc variadic trampolines (ticket C59).
//!
//! The library is the C59 module compiled by path from the shell crate — the shell skeleton
//! did not exist when the ticket started (RD-new 2), so this crate is what builds the module
//! for the bare-metal targets:
//!
//! ```text
//! cargo build -p raven-libc-va-test --target x86_64-unknown-none
//! ```
//!
//! Everything else lives in the integration tests: the ABI model (`src/model.rs`), the arena
//! (`src/arena.rs`), the generated-C driver (`src/driver.rs`), the test targets
//! (`src/targets.rs`) and the one expansion of the variadic table (`src/table_ut.rs`). Those
//! modules need `std` and are included by path from the tests, never by this library.
#![cfg_attr(not(test), no_std)]
#![allow(unexpected_cfgs)] // `raven_va_softfloat` is a documented build-profile cfg

#[path = "../../../../libc/ravenlibc/crates/raven-libc/src/va/mod.rs"]
pub mod va;

// The whole table, expanded once, so that the bare-metal build check also assembles the
// trampolines (or their link-time failing form) for the target.
#[cfg(not(test))]
#[path = "va_stubs.rs"]
pub mod va_stubs;
