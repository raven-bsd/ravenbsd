// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel memory management.
//!
//! The frame allocator, page tables, heap and the MMIO window are compiled for both
//! architectures (S14a ported them to aarch64). The user address-space code is still x86-64
//! only; user mode on aarch64 lands in S14c.

#[cfg(target_arch = "x86_64")]
pub mod aspace;
#[allow(dead_code)] // The alloc/free API is consumed by S05/S06.
pub mod frame;
pub mod heap;
pub mod mmio;
pub mod paging;
#[cfg(target_arch = "x86_64")]
pub mod uspace;
