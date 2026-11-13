// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Secret scrubbing for the `crypt` paths.
//!
//! `raven-libc-str` exposes `explicit_bzero` as an `unsafe` raw function; this crate is class P
//! (`forbid(unsafe_code)`) and cannot call it. The safe equivalent below zeroes the buffer and
//! passes it through `core::hint::black_box`, which forces the zeroed value to be materialised
//! and keeps the stores from being eliminated as dead. The C65 report records the deviation.

/// Zero a byte buffer that held key material.
pub fn scrub(buf: &mut [u8]) {
    buf.fill(0);
    let _ = core::hint::black_box(buf);
}

/// Zero a word buffer (the Blowfish key schedule).
pub fn scrub_u32(buf: &mut [u32]) {
    buf.fill(0);
    let _ = core::hint::black_box(buf);
}

#[cfg(test)]
#[path = "zero_ut.rs"]
mod tests;
