// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `.raven_unimpl` record and the `-1`/`errno` stub body.
//!
//! Each `unimplemented_symbol!` writes one fixed-layout [`UnimplRecord`] into its own section.
//! `xtask libc-pack` (C66) reads the section and generates `<raven/stubs.h>`; the section is in
//! its own archive member, so a program that does not reference the stub does not pull it in.

#![allow(clippy::arithmetic_side_effects)] // const string packing

/// Length of the function-name field, including the NUL.
pub const NAME_LEN: usize = 64;
/// Length of the gap-id field, including the NUL.
pub const GAP_LEN: usize = 16;
/// Magic identifying a record produced by this crate.
pub const MAGIC: u32 = 0x554E_494D; // "UNIM"

/// One class-A unimplemented-symbol record.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnimplRecord {
    /// Record magic, [`MAGIC`].
    pub magic: u32,
    /// Positive errno the stub sets.
    pub errno: i32,
    /// NUL-terminated `name`.
    pub name: [u8; NAME_LEN],
    /// NUL-terminated gap id (`G-n`).
    pub gap: [u8; GAP_LEN],
}

impl UnimplRecord {
    /// Build a record, truncating and NUL-terminating both strings.
    pub const fn new(name: &'static str, gap: &'static str, errno: i32) -> Self {
        Self {
            magic: MAGIC,
            errno,
            name: copy_str::<NAME_LEN>(name),
            gap: copy_str::<GAP_LEN>(gap),
        }
    }

    /// The function name as a string slice.
    pub fn name_str(&self) -> &str {
        bytes_to_str(&self.name)
    }

    /// The gap id as a string slice.
    pub fn gap_str(&self) -> &str {
        bytes_to_str(&self.gap)
    }
}

#[allow(clippy::indexing_slicing)] // fixed-size const packing with explicit bounds
const fn copy_str<const N: usize>(text: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && index + 1 < N {
        out[index] = bytes[index];
        index += 1;
    }
    out
}

fn bytes_to_str(bytes: &[u8]) -> &str {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    bytes
        .get(..end)
        .and_then(|slice| core::str::from_utf8(slice).ok())
        .unwrap_or_default()
}

/// Body shared by every generated stub: store `errno` and return `-1`.
pub fn fail(errno: i32) -> i32 {
    crate::errno::set_errno(errno);
    -1
}

#[cfg(test)]
#[path = "unimpl_ut.rs"]
mod tests;
