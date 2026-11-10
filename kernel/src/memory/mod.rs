// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel memory management.
//!
//! The frame allocator is architecture-neutral. Paging, the heap and the user address-space code
//! are still x86-64 only; the aarch64 MMU port lands in S14a.

#[cfg(target_arch = "x86_64")]
pub mod aspace;
#[allow(dead_code)] // The alloc/free API is consumed by S05/S06.
pub mod frame;
#[cfg(target_arch = "x86_64")]
pub mod heap;
#[cfg(target_arch = "x86_64")]
pub mod mmio;
#[cfg(target_arch = "x86_64")]
pub mod paging;
#[cfg(target_arch = "x86_64")]
pub mod uspace;
