// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side U4 user-management matrix.
//!
//! The crate carries no production code. Its unit tests drive the U2 tool argument layers and the
//! `raven-userdb` operations together against the canonical fixtures and the 0.1.0 image files.
//! The target `qtest` matrix is deferred until U3b lands setuid and non-root credentials.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
