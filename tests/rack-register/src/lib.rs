// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only RACK 219-row acceptance register repair and validation.
//!
//! The crate proposes one owner, Rust mapping, named test, mutation and PASS status per donor
//! function, and checks the committed register against the current product source. No product
//! code is edited.

#![forbid(unsafe_code)]

pub mod check;
pub mod delegate;
pub mod propose;
pub mod source;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
