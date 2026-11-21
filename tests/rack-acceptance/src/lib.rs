// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only RACK 218/218 acceptance audit.
//!
//! This crate is evidence tooling, not product code. It verifies the canonical K315 register
//! against the current `raven-tcpip` source, runs the K337 replay and names the owning ticket for
//! every missing mapping, test or mutation. No product source is edited.

#![forbid(unsafe_code)]

pub mod audit;
pub mod register;
pub mod reports;
pub mod source;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
