// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Virtual-memory layout constants used by the loader.
//!
//! `raven-abi::layout::aarch64` does not exist yet; S12 defines the same values here and the lead
//! can promote them to the contract crate later. Both architectures use the same kernel image
//! base so the kernel linker script shape is shared.

/// Direct map of all physical RAM.
#[cfg(target_arch = "x86_64")]
pub const HHDM_BASE: u64 = raven_abi::layout::x86_64::HHDM_BASE;

/// Direct map of all physical RAM.
#[cfg(target_arch = "aarch64")]
pub const HHDM_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Virtual base of the kernel image (top 2 GiB).
pub const KERNEL_IMAGE_BASE: u64 = 0xFFFF_FFFF_8000_0000;

/// Range accepted for kernel `PT_LOAD` segments.
pub const KERNEL_RANGE: core::ops::Range<u64> = KERNEL_IMAGE_BASE..0xFFFF_FFFF_FFFF_F000;
