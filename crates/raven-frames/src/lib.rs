// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! No-allocation bitmap physical frame allocator.
//!
//! The implementation lives in [`imp`] so the kernel can compile the same source directly with
//! `#[path]` while this crate remains the host-testable API.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

mod imp;

pub use imp::*;
