// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target stubs selected by `raw/mod.rs`; the per-architecture modules are re-exported.

#[cfg(target_arch = "x86_64")]
pub use crate::raw::arch::entry;
