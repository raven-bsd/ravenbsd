// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Disassembly probes for the `test-probes` feature (ticket 5.4(c)).
//!
//! `probe_zero_with_barrier` must keep all 64 stores; `probe_zero_without_barrier` may be
//! dead-store eliminated. The structural test compares the store counts of the two functions.

/// Fill a 64-byte local with `0xAA`, zero it through `explicit_bzero`, return one byte.
#[inline(never)]
pub fn probe_zero_with_barrier() -> u8 {
    let mut buffer = [0xAAu8; 64];
    // SAFETY: `buffer` is a live 64-byte local, writable for its whole length.
    unsafe {
        super::explicit_bzero::explicit_bzero(buffer.as_mut_ptr(), buffer.len());
    }
    buffer[0]
}

/// Fill a 64-byte local with `0xAA`, zero it with a plain fill, return one byte.
#[inline(never)]
pub fn probe_zero_without_barrier() -> u8 {
    let mut buffer = [0xAAu8; 64];
    buffer.fill(0);
    buffer[0]
}
