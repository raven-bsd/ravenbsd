// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only independent model for RACK-TLP, DSACK reordering growth, spurious undo and
//! SACK reneging. This crate is an oracle for the future full `Rack` port; it never edits
//! product source.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

pub mod model;
pub mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
