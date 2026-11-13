// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// CLEANROOM: the behaviour is the ticket's GNU basename contract and its vectors.
// The bionic BSD-2 implementation was read only to confirm the trailing-slash rule;
// bionic is not a donors.lock entry, so the file claims no import and reproduces no
// donor text. The implementation below is an independent iterator formulation.

//! GNU `basename` semantics, exported as `__gnu_basename` (A-str-8).

/// Offset of the last path component: one past the last `/`, or 0 when there is none.
///
/// A trailing slash therefore yields the offset of the terminating NUL (the empty string), as the
/// GNU function does. The POSIX `basename` (which strips trailing slashes) belongs to C41.
#[must_use]
pub fn offset(path: &[u8]) -> usize {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |index| index.wrapping_add(1))
}

#[cfg(test)]
#[path = "gnu_basename_ut.rs"]
mod tests;
