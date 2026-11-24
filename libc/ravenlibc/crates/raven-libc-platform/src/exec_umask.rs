// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The libc-only `umask` hand-off across `execve`.
//!
//! The Raven kernel keeps no per-process umask (`grep umask kernel/src` is empty), so the mask is
//! process state in [`crate::platform::Platform`]. `execve` replaces the image and therefore the
//! old platform object; the BSD donor keeps the mask in the process credential (`p_pd->pd_cmask`,
//! `kern_descrip.c:2707`) where exec cannot clear it. Raven carries the same value through the
//! established exec environment metadata (the `__RAVEN_FDMAP` hand-off), and the new image's
//! platform startup consumes and removes it.
//!
//! This is process state, never a user-facing variable: [`encode`] writes the value and
//! [`decode`] parses the four-digit octal mask.

use alloc::vec::Vec;

/// Environment name used for the exec hand-off only.
pub const ENV_NAME: &[u8] = b"__RAVEN_UMASK";

/// Encode `mask` as a `NAME=0NNN` environment entry.
pub fn encode(mask: u32) -> Vec<u8> {
    let mask = mask & 0o777;
    let mut out = Vec::with_capacity(ENV_NAME.len() + 1 + 4);
    out.extend_from_slice(ENV_NAME);
    out.push(b'=');
    for shift in [9u32, 6, 3, 0] {
        out.push(b'0' + ((mask >> shift) & 7) as u8);
    }
    out
}

/// Parse the value half of the entry; `None` for anything that is not 1..=4 octal digits.
pub fn decode(value: &[u8]) -> Option<u32> {
    if value.is_empty() || value.len() > 4 {
        return None;
    }
    let mut mask = 0u32;
    for byte in value {
        if !(b'0'..=b'7').contains(byte) {
            return None;
        }
        mask = (mask << 3) | u32::from(byte - b'0');
    }
    Some(mask & 0o777)
}

#[cfg(test)]
#[path = "exec_umask_ut.rs"]
mod tests;
