// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `a64l`, `l64a` and `getsubopt`.

pub mod a64l;
pub mod getsubopt;
pub mod l64a;

pub use a64l::a64l;
pub use getsubopt::{Subopt, getsubopt};
pub use l64a::l64a;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
