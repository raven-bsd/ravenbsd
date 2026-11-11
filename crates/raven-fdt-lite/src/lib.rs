// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! No-allocation reader for flattened device trees.
//!
//! The implementation lives in [`imp`] so the kernel can compile the same source directly with
//! `#[path]` while this crate remains the host-testable API. Every accessor returns `Option` or
//! `Result`; the reader never panics, whatever bytes the firmware hands over.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

mod imp;

pub use imp::*;
