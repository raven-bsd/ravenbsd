// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The 0/1/2 exit status contract.

/// The only exit statuses the modern command contract defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Exit {
    /// The command succeeded; for a mutation the requested state is present.
    Success = 0,
    /// The command ran and the result is a failure or a degraded state.
    Failure = 1,
    /// The command line is invalid: unknown flag, missing argument, or a flag conflict.
    Usage = 2,
}

impl Exit {
    /// The process exit code.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}
