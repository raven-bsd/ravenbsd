// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX.1-2024 profile validation and gap-matrix generation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod gate;
pub mod mgmt;
pub mod model;
pub mod overlay;
pub mod render;
pub mod seed;
pub mod sweep;

#[cfg(test)]
#[path = "gate_ut.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "model_ut.rs"]
mod model_tests;

#[cfg(test)]
#[path = "overlay_ut.rs"]
mod overlay_tests;

#[cfg(test)]
#[path = "sweep_ut.rs"]
mod sweep_tests;
