// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable P2f process-primitive encodings.
//!
//! The kernel compiles the same `status.rs`; this library exists so the wait
//! status, signal numbers and rlimit defaults have host tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[path = "../../../../kernel/src/process/status.rs"]
pub mod status;
