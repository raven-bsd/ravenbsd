// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `system()`: the signal/wait protocol over a spawn backend.

pub mod system;

pub use system::{Sig, SystemBackend, system};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
