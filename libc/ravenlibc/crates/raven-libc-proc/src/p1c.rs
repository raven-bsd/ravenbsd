// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thin P1c process wrappers over [`ProcBackend`].

use crate::backend::ProcBackend;
use crate::errors::Errno;

/// `setpgrp`: put the caller in a process group named by its pid and return that group.
///
/// # Errors
///
/// Returns the backend's `setpgid` or `getpgid` error.
pub fn setpgrp<B: ProcBackend>(backend: &B) -> Result<i32, Errno> {
    backend.setpgid(0, 0)?;
    backend.getpgid(0)
}

#[cfg(test)]
#[path = "p1c_ut.rs"]
mod tests;
