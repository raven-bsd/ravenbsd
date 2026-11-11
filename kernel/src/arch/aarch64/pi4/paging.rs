// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared page-table builder for the Pi 4 entry stub.
//!
//! The implementation lives in `raven-bootinfo-fdt` so it has host tests; the kernel compiles the
//! same source with `#[path]`. The generic builder is inlined into the low entry code (see
//! [`super::early`]); the module mirrors the crate's public surface.

#[path = "../../../../../crates/raven-bootinfo-fdt/src/paging.rs"]
#[allow(dead_code)] // Shared crate source includes helpers used by its own host tests.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
pub mod imp;
