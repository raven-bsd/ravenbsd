// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX.1-2024 profile validation and gap-matrix generation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod model;
pub mod render;
pub mod seed;

#[cfg(test)]
#[path = "model_ut.rs"]
mod model_tests;
