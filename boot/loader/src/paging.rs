// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Architecture-selected page-table builder.
//!
//! x86-64 uses 4-level page tables loaded through CR3. aarch64 uses two roots: TTBR0 for the
//! identity-mapped loader and TTBR1 for the HHDM plus kernel image. Both implement the
//! [`crate::paging_core`] contract and have host unit tests.

#[cfg(target_os = "uefi")]
pub use crate::paging_core::{LoaderMap, UefiPhysMem};
pub use crate::paging_core::{
    MAX_PHYSICAL_BITS, MIN_PHYSICAL_BITS, PAGE_SIZE, PAGE_SIZE_1G, PAGE_SIZE_2M, PageError, PhysMem,
};

#[cfg(target_arch = "x86_64")]
pub use crate::arch::x86_64::paging::*;

#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64::paging::*;
