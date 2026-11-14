// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `pledge`/`unveil` entry glue (`B03`, `10` 3.9.3 phase 1, `15` 3.A.6).
//!
//! The grammar, the 35 promises, the monotonic reduction and the 128-entry unveil bound are
//! C53's `raven-libc-pledge`; this module only decodes the arguments, calls the model, maps its
//! error to `errno` (`EINVAL` unknown promise, `EPERM` increase or unveil after lock, `E2BIG`
//! above 128 entries) and calls [`PledgeBackend`]. The default backend stores nothing and
//! returns `Ok` (phase 1: `PG-39` enforcement is absent). `NULL, NULL` is a no-op returning 0.

use raven_libc_pledge::reduce::PledgeState;
use raven_libc_pledge::unveil::UnveilTable;

use crate::seams::{Errno, PledgeBackend};

fn map(error: i32) -> Errno {
    Errno(u16::try_from(error).unwrap_or(Errno::EINVAL.0))
}

/// `pledge(promises, execpromises)`.
pub fn pledge<B: PledgeBackend>(
    backend: &mut B,
    state: &mut PledgeState,
    promises: Option<&[u8]>,
    exec: Option<&[u8]>,
) -> Result<(), Errno> {
    if promises.is_none() && exec.is_none() {
        return Ok(());
    }
    state.apply(promises, exec).map_err(map)?;
    backend.apply(promises, exec)
}

/// `unveil(path, perms)`.
pub fn unveil<B: PledgeBackend>(
    backend: &mut B,
    state: &mut UnveilTable,
    path: Option<&[u8]>,
    perms: Option<&[u8]>,
) -> Result<(), Errno> {
    state.unveil(path, perms).map_err(map)?;
    backend.unveil(path, perms)
}

#[cfg(test)]
#[path = "pledge_glue_ut.rs"]
mod tests;
