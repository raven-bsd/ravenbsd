// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The shell's one process backend (C64 4.9): `LocalBackend` from C53 until `procd` exists.
//!
//! C53 ships no `ProcdBackend`, and the saved auxiliary vector is owned by C61's start block
//! without an accessor on this branch; the choice is therefore a lazy function returning the
//! pre-`procd` backend. The first `AT_RAVEN_PROCD` backend is a follow-up question recorded in
//! the report. Nothing is created at start and `fork`/`exec`/`spawn` report the backend's
//! `ENOSYS` honestly (H-18).
//!
//! `LocalBackend` has no interior state, so constructing it per call is the same as one
//! process-wide value and needs no registration (R-9/R-28).

use raven_libc_proc::local_backend::LocalBackend;

/// The process backend of the shell.
pub(crate) fn backend() -> LocalBackend<'static> {
    LocalBackend::new(None)
}

/// Whether a `procd` connection was handed over (`false` on this branch).
pub(crate) fn procd_present() -> bool {
    false
}
