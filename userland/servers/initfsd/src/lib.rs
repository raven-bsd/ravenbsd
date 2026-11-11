// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `initfs:` read-only filesystem server library.
//!
//! The pure tree model lives in [`model`]; the target binary maps the kernel-installed initfs
//! handle 1, parses it with `raven-initfs`, and serves the model through K30/K60.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod model;
