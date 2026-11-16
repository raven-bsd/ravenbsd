// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The getrandom 0.4 custom backend for RavenBSD.
//!
//! `getrandom 0.4.3` selects its `custom` backend with
//! `--cfg getrandom_backend="custom"`; the final binary must define
//! `__getrandom_v03_custom`. The backend reads the `rand:` scheme, which is the first-party
//! pseudo-random device (seeded from the monotonic clock and a stack address; not yet a
//! cryptographic entropy source). PG-52/G-7 replaces it with the kernel CSPRNG.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // the `no_mangle` ABI export and destination slice
#![allow(unexpected_cfgs)] // `ravenbsd` is a custom target known only to the ravenbsd toolchain

/// Human-readable backend status for diagnostics.
pub const BACKEND: &str = "ravenbsd: rand: scheme";

/// The getrandom 0.4 custom backend entry point.
#[cfg(target_os = "ravenbsd")]
#[unsafe(no_mangle)]
pub extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    if len == 0 {
        return Ok(());
    }
    let out = unsafe { core::slice::from_raw_parts_mut(dest, len) };
    let client = raven_scheme::Client::new();
    let file = client
        .open("rand:", 1)
        .map_err(|_| getrandom::Error::UNSUPPORTED)?;
    let mut filled = 0usize;
    while filled < out.len() {
        let Some(tail) = out.get_mut(filled..) else {
            let _ = client.close(file);
            return Err(getrandom::Error::UNSUPPORTED);
        };
        match client.read(file, 0, tail) {
            Ok(0) => {
                let _ = client.close(file);
                return Err(getrandom::Error::UNSUPPORTED);
            }
            Ok(count) => filled = filled.saturating_add(count),
            Err(_) => {
                let _ = client.close(file);
                return Err(getrandom::Error::UNSUPPORTED);
            }
        }
    }
    let _ = client.close(file);
    Ok(())
}
