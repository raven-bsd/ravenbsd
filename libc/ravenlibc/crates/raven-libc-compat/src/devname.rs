// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `devname` stub (donor-map 15 T3, 02 R32): returns `NULL`.
//!
//! There is no donor code; the census row is `stub:null`. `ls -l` of a device node is the only
//! consumer, and Raven resolves device names in `devd`.

/// `devname`: the NULL stub.
#[must_use]
pub fn devname_stub() -> Option<&'static [u8]> {
    None
}

#[cfg(test)]
#[path = "devname_ut.rs"]
mod tests;
