// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `basename` and `dirname` results over a caller's buffer.
//!
//! The FreeBSD donors modify the argument in place and return a pointer into it (or the static
//! `"."`). The pure engine returns the resulting `(start, len)` prefix, or [`GenName::Static`]
//! for `"."`; the shell writes the NUL and returns the pointer (`<libgen.h>` does `#undef
//! basename`, CR-9). The OpenBSD static-buffer versions are rejected (not thread safe).

pub use super::basename::basename;
pub use super::dirname::dirname;

/// The result of an in-place path operation.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum GenName {
    /// The static string `"."` (or another donor constant).
    Static(&'static [u8]),
    /// The `len` bytes of the caller's buffer starting at `start`.
    Slice {
        /// Start index.
        start: usize,
        /// Byte length.
        len: usize,
    },
}
