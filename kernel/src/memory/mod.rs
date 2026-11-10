// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel memory management.

pub mod aspace;
#[allow(dead_code)] // The alloc/free API is consumed by S05/S06.
pub mod frame;
pub mod heap;
pub mod mmio;
pub mod paging;
pub mod uspace;
