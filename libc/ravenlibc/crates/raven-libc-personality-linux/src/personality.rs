// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `personality` (`10` 3.8, `X86`).
//!
//! Raven is one personality: Linux `PER_LINUX` (`0`). The query `0xffffffff` reports it and
//! setting it is accepted; everything else is `EINVAL`. The symbol exists so autoconf probes
//! link and take their non-Linux branch at run time.

use crate::seams::Errno;

/// `PER_LINUX`.
pub const PER_LINUX: u64 = 0;
/// The Linux query word (`0xffffffff`).
pub const PERSONALITY_QUERY: u64 = 0xffff_ffff;

/// `personality(persona)`.
pub fn personality(persona: u64) -> Result<i64, Errno> {
    if persona == PERSONALITY_QUERY || persona == u64::from(u32::MAX) || persona == PER_LINUX {
        Ok(0)
    } else {
        Err(Errno::EINVAL)
    }
}

#[cfg(test)]
#[path = "personality_ut.rs"]
mod tests;
