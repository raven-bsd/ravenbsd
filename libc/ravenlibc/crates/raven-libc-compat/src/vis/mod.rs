// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `vis`/`unvis` (donor-map 15 3.B table T3, 02 B10, doc 15 4 "vis").

pub mod flags;
pub mod unvis;
#[allow(clippy::module_inception)] // the C header names the module `vis`
pub mod vis;

pub use flags::*;
pub use unvis::{Unvis, UnvisStep, strnunvis, strnunvisx, strunvis, strunvisx};
pub use vis::{
    VIS_MAX_EXPANSION, nvis, snvis, strnvis, strnvisx, strsnvis, strsnvisx, strsvis, strsvisx,
    strvis, strvisx, svis, vis,
};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
