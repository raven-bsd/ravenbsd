// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `realpath` over a caller-supplied filesystem walker.

pub mod realpath;

pub use realpath::{FileKind, MAXSYMLINKS, Walker, realpath};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
