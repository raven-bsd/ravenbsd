// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc C-ABI shell.
//!
//! This crate owns every `extern "C"` export of ravenlibc, the `global_asm!` variadic
//! trampolines and the `va_list` readers. At ticket C59 it carries only the variadic module
//! [`va`]: the shell skeleton exists so that the module has a home and so that
//! `cargo build -p raven-libc --target <arch>-unknown-none` is a real build check. C60 fills
//! the export modules, `Symbol.map`, `src/start.rs` and the panic handler.
//!
//! Class `U` of `17-crate-layout.md` section 1 (L-3): the shell may use `unsafe`; every block
//! carries a `// SAFETY:` comment (workspace lint `undocumented_unsafe_blocks`).
#![cfg_attr(not(test), no_std)]
#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are documented build-profile cfgs
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

pub mod va;
