// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Architecture-specific kernel support.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

/// Architecture name printed in the C4 boot banner.
#[cfg(target_arch = "x86_64")]
pub const ARCH_NAME: &str = "x86_64";
/// Architecture name printed in the C4 boot banner.
#[cfg(target_arch = "aarch64")]
pub const ARCH_NAME: &str = "aarch64";

/// Virtual address where physical address 0 is mapped.
///
/// Each architecture reads the value from its `raven-abi` layout module, so `boot::validate`
/// compares the loader's `BootInfo::hhdm_offset` against the contract of the running target.
#[cfg(target_arch = "x86_64")]
pub const HHDM_BASE: u64 = raven_abi::layout::x86_64::HHDM_BASE;
/// Virtual address where physical address 0 is mapped.
#[cfg(target_arch = "aarch64")]
pub const HHDM_BASE: u64 = raven_abi::layout::aarch64::HHDM_BASE;
