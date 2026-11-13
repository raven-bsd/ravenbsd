// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/gen/posix_spawnp.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01

//! The `posix_spawnp` prechecks.

use crate::errors::{ENOENT, Errno};

/// Decide whether `file` needs a `PATH` search.
///
/// An empty name is `ENOENT`; a name containing `/` is tried directly; every other name is
/// searched by [`crate::exec_path::search_path`].
pub fn spawnp_prechecks(file: &[u8]) -> Result<bool, Errno> {
    if file.is_empty() {
        return Err(ENOENT);
    }
    Ok(!file.contains(&b'/'))
}

#[cfg(test)]
#[path = "spawnp_ut.rs"]
mod tests;
