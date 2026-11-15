// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! A target-only compile probe for the `getrandom` custom backend and the
//! patched `libc` 0.2.185 ravenbsd module. Building this crate for
//! `x86_64-unknown-ravenbsd` proves that both dependency layers are usable by
//! ordinary Rust crates; the final brush link is K26a's job.
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// The backend status exposed by `raven-getrandom`.
pub use raven_getrandom::BACKEND;

/// Fill `buffer` through `getrandom`'s selected backend.
pub fn fill(buffer: &mut [u8]) -> Result<(), getrandom::Error> {
    getrandom::fill(buffer)
}

/// Touch the patched libc ravenbsd types so the crate really compiles them.
#[must_use]
pub fn libc_types() -> (libc::time_t, libc::iovec) {
    (
        0,
        libc::iovec {
            iov_base: core::ptr::null_mut(),
            iov_len: 0,
        },
    )
}
