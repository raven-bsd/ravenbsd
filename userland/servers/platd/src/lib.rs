// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable parts of `platd`.
//!
//! The server itself is a `no_std` program; the capability argument parser lives here so
//! `cargo nextest run -p raven-platd` can validate it on the host.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod resources;
