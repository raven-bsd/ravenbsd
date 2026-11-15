// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The getrandom 0.4 custom backend for RavenBSD.
//!
//! `getrandom 0.4.3` selects its `custom` backend with
//! `--cfg getrandom_backend="custom"`; the final binary must define
//! `__getrandom_v03_custom`. The kernel entropy primitive (`G-7`) is not
//! available yet, so this backend returns `Error::UNSUPPORTED` instead of
//! fabricating bytes. The symbol exists so a Rust crate that only links
//! `getrandom` builds; a caller that needs entropy sees a documented error.
#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // the `no_mangle` ABI export
#![allow(unexpected_cfgs)] // `ravenbsd` is a custom target known only to the ravenbsd toolchain

/// Human-readable backend status for diagnostics.
pub const BACKEND: &str = "ravenbsd: unavailable (G-7)";

/// The getrandom 0.4 custom backend entry point.
#[cfg(target_os = "ravenbsd")]
#[unsafe(no_mangle)]
pub extern "Rust" fn __getrandom_v03_custom(
    _dest: *mut u8,
    _len: usize,
) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}
