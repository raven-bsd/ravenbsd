// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only audit of the merged independent RACK model group.
//!
//! The audit pins donor functions/values, model crate/source/token, named test, mutation evidence
//! and the K315 manifest row, then checks the committed files. It does not edit product source.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

pub mod audit;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
