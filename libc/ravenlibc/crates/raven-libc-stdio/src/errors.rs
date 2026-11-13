// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The crate result type and the C return-value classes of `donor-map/04` 4.4.
//!
//! The crate never touches `errno`: it returns [`Fail::Eof`] for the C cases that return
//! `EOF`/`0`/`NULL` with `errno` untouched, and [`Fail::Errno`] with the value the shell must
//! store. Every `Fail::Errno` is produced inside the crate.

use raven_libc_abi::errno::Errno;

/// The C end-of-file marker.
pub const EOF: i32 = -1;

/// The two failure classes of the C API.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fail {
    /// C returns `EOF`, `0`, `NULL` or a short count; `errno` stays as it was.
    Eof,
    /// C returns `EOF`, `0` or `NULL` and the shell sets `errno` to this value.
    Errno(Errno),
}

/// The crate result type.
pub type R<T> = Result<T, Fail>;

impl Fail {
    /// Build an errno failure from one of the generated `E*` integer constants.
    #[must_use]
    pub const fn from_int(code: i32) -> Self {
        Self::Errno(Errno(code as u16))
    }
}

impl From<Errno> for Fail {
    fn from(value: Errno) -> Self {
        Self::Errno(value)
    }
}

impl Fail {
    /// The `errno` value of the failure, or `None` for [`Fail::Eof`].
    #[must_use]
    pub const fn errno(self) -> Option<Errno> {
        match self {
            Self::Eof => None,
            Self::Errno(value) => Some(value),
        }
    }
}

#[cfg(test)]
#[path = "errors_ut.rs"]
mod tests;
