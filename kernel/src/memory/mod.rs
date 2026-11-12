// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel memory management.
//!
//! The frame allocator, page tables, heap, MMIO window and the user address-space code compile
//! for both architectures; the user mapping layer selects the architecture's page encoding.

pub mod aspace;
#[allow(dead_code)] // The alloc/free API is consumed by S05/S06.
pub mod frame;
pub mod heap;
pub mod mmio;
#[cfg(target_arch = "x86_64")]
pub mod pager;
pub mod paging;
pub mod uspace;
