// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only independent model of the FreeBSD `tcp_function_block` RACK vtable.
//!
//! This crate is test/evidence tooling, not product code. It models the 19 `tfb_*` callbacks
//! rack.c installs, their lifecycle, ordering, return contracts and the named limits. K319/K320
//! own the RACK core (loss/TLP/DSACK); this crate deliberately does not duplicate those algorithms.
//! The stable trace line is documented in `docs/rack-vtable-model.md`.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

pub mod model;
pub mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
